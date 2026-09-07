mod audio;
mod metrics;

use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand, ValueEnum};
use noise_net::{ALGORITHM_LATENCY_SAMPLES, FRAME_SIZE, NoiseNet, SAMPLE_RATE};
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(
    about = "Prepare known speech/noise mixtures, run NoiseNet, and score external processors"
)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Generate immutable mono 48 kHz WAV triplets and their SHA-256 manifest.
    Prepare {
        #[arg(long, required = true)]
        speech: Vec<PathBuf>,
        #[arg(long, required = true)]
        noise: Vec<PathBuf>,
        #[arg(long)]
        output: PathBuf,
        /// Explicit source channel (1 = left). Omit only for mono sources.
        #[arg(long)]
        channel: Option<u16>,
        /// Recorded sources may contain background noise; do not call these clean ground truth.
        #[arg(long, value_enum, default_value = "recorded")]
        reference_kind: ReferenceKind,
        /// SNR uses whole-clip energy, including pauses. Both components share any headroom gain.
        #[arg(
            long,
            value_delimiter = ',',
            allow_hyphen_values = true,
            default_value = "0,10,20"
        )]
        snr_db: Vec<f32>,
        #[arg(
            long,
            value_delimiter = ',',
            allow_hyphen_values = true,
            default_value = "0,-12,-24"
        )]
        gain_db: Vec<f32>,
        /// Sample offset into the cyclic noise recording; saved for reproducibility.
        #[arg(long, default_value_t = 13700)]
        noise_offset: usize,
    },
    /// Process the exact manifest inputs. Outputs are aligned by the known algorithmic delay.
    Run {
        #[arg(long)]
        suite: PathBuf,
        #[arg(long)]
        output: PathBuf,
        #[arg(long, value_enum, default_value = "product")]
        mode: Mode,
        #[arg(long, default_value = "cpu")]
        processor: String,
        /// Carry all recurrent and adaptation state across consecutive cases.
        #[arg(long)]
        continuous: bool,
    },
    /// Score already recorded AMD/other outputs named `<case-id>.wav` without gain normalization.
    Score {
        #[arg(long)]
        suite: PathBuf,
        #[arg(long)]
        recordings: PathBuf,
        #[arg(long)]
        output: PathBuf,
        /// Record the external software version, processor and settings here.
        #[arg(long)]
        label: String,
        /// Explicit measured delay. No automatic correlation or time stretching is applied.
        #[arg(long, default_value_t = 0)]
        delay_samples: usize,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum ReferenceKind {
    Clean,
    Recorded,
}
#[derive(Clone, Copy, Debug, Serialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
enum Mode {
    Product,
    Reference,
    Passthrough,
}

#[derive(Deserialize, Serialize)]
struct Source {
    path: PathBuf,
    sha256: String,
    /// One-based channel selected without downmixing. None means a mono source.
    #[serde(default)]
    channel: Option<u16>,
}
#[derive(Deserialize, Serialize)]
struct Artifact {
    file: String,
    sha256: String,
}
#[derive(Deserialize, Serialize)]
struct Case {
    id: String,
    kind: String,
    speech_source: Option<usize>,
    noise_source: Option<usize>,
    gain_db: f32,
    snr_db: Option<f32>,
    headroom_gain: f32,
    samples: usize,
    input: Artifact,
    speech: Artifact,
    noise: Artifact,
}
#[derive(Deserialize, Serialize)]
struct Suite {
    schema: u32,
    sample_rate: u32,
    reference_kind: ReferenceKind,
    speech_sources: Vec<Source>,
    noise_sources: Vec<Source>,
    noise_offset: usize,
    cases: Vec<Case>,
}
#[derive(Serialize)]
struct ResultRow {
    id: String,
    kind: String,
    scores: metrics::Scores,
    output_sha256: String,
}
#[derive(Default, Serialize)]
struct Timing {
    frames: usize,
    mean_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
    deadline_misses: usize,
}
#[derive(Serialize)]
struct Report {
    schema: u32,
    label: String,
    suite_sha256: String,
    reference_kind: ReferenceKind,
    continuous: Option<bool>,
    delay_samples: usize,
    timing: Option<Timing>,
    rows: Vec<ResultRow>,
}

fn main() -> Result<()> {
    match Args::parse().command {
        Command::Prepare {
            speech,
            noise,
            output,
            channel,
            reference_kind,
            snr_db,
            gain_db,
            noise_offset,
        } => prepare(
            &speech,
            &noise,
            &output,
            reference_kind,
            &snr_db,
            &gain_db,
            noise_offset,
            channel,
        ),
        Command::Run {
            suite,
            output,
            mode,
            processor,
            continuous,
        } => run(&suite, &output, mode, &processor, continuous),
        Command::Score {
            suite,
            recordings,
            output,
            label,
            delay_samples,
        } => {
            let manifest = load_suite(&suite)?;
            fs::create_dir(&output).context("output directory must not already exist")?;
            let report = collect_scores(
                &suite,
                &manifest,
                &recordings,
                label,
                None,
                delay_samples,
                None,
            )?;
            write_report(&output, &report)
        }
    }
}

fn save_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}
fn source(path: &Path, channel: Option<u16>) -> Result<Source> {
    Ok(Source {
        path: fs::canonicalize(path)?,
        sha256: audio::hash(path)?,
        channel,
    })
}
fn artifact(dir: &Path, name: String, samples: &[f32]) -> Result<Artifact> {
    let path = dir.join(&name);
    audio::write(&path, samples)?;
    Ok(Artifact {
        file: name,
        sha256: audio::hash(&path)?,
    })
}

#[allow(clippy::too_many_arguments)]
fn add_case(
    suite: &mut Suite,
    dir: &Path,
    kind: &str,
    speech_source: Option<usize>,
    noise_source: Option<usize>,
    gain_db: f32,
    snr_db: Option<f32>,
    headroom_gain: f32,
    speech: &[f32],
    noise: &[f32],
) -> Result<()> {
    let id = format!("case-{:04}", suite.cases.len());
    let input = speech
        .iter()
        .zip(noise)
        .map(|(s, n)| s + n)
        .collect::<Vec<_>>();
    ensure!(
        input.iter().all(|v| v.is_finite() && v.abs() <= 1.0),
        "{id} clips; reduce source gain"
    );
    suite.cases.push(Case {
        input: artifact(dir, format!("{id}-input.wav"), &input)?,
        speech: artifact(dir, format!("{id}-speech.wav"), speech)?,
        noise: artifact(dir, format!("{id}-noise.wav"), noise)?,
        id,
        kind: kind.into(),
        speech_source,
        noise_source,
        gain_db,
        snr_db,
        headroom_gain,
        samples: speech.len(),
    });
    Ok(())
}

#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn prepare(
    speech_paths: &[PathBuf],
    noise_paths: &[PathBuf],
    out: &Path,
    reference_kind: ReferenceKind,
    snrs: &[f32],
    gains: &[f32],
    noise_offset: usize,
    channel: Option<u16>,
) -> Result<()> {
    ensure!(
        snrs.iter()
            .all(|v| v.is_finite() && (-40.0..=60.0).contains(v)),
        "SNR must be -40..60 dB"
    );
    ensure!(
        gains
            .iter()
            .all(|v| v.is_finite() && (-80.0..=0.0).contains(v)),
        "gain must be -80..0 dB"
    );
    let voices = speech_paths
        .iter()
        .map(|p| audio::read_channel(p, channel))
        .collect::<Result<Vec<_>>>()?;
    let noises = noise_paths
        .iter()
        .map(|p| audio::read_channel(p, channel))
        .collect::<Result<Vec<_>>>()?;
    fs::create_dir(out).context("suite directory must not already exist")?;
    let mut suite = Suite {
        schema: 1,
        sample_rate: SAMPLE_RATE,
        reference_kind,
        speech_sources: speech_paths
            .iter()
            .map(|p| source(p, channel))
            .collect::<Result<_>>()?,
        noise_sources: noise_paths
            .iter()
            .map(|p| source(p, channel))
            .collect::<Result<_>>()?,
        noise_offset,
        cases: vec![],
    };
    for (si, voice) in voices.iter().enumerate() {
        for &gain_db in gains {
            let scaled = voice
                .iter()
                .map(|v| v * 10.0_f32.powf(gain_db / 20.0))
                .collect::<Vec<_>>();
            add_case(
                &mut suite,
                out,
                "voice",
                Some(si),
                None,
                gain_db,
                None,
                1.0,
                &scaled,
                &vec![0.0; voice.len()],
            )?;
            for (ni, noise) in noises.iter().enumerate() {
                let cyclic = (0..voice.len())
                    .map(|i| noise[(i % noise.len() + noise_offset % noise.len()) % noise.len()])
                    .collect::<Vec<_>>();
                for &snr_db in snrs {
                    let (s, n, headroom) = audio::mix(voice, &cyclic, gain_db, snr_db)?;
                    add_case(
                        &mut suite,
                        out,
                        "mixture",
                        Some(si),
                        Some(ni),
                        gain_db,
                        Some(snr_db),
                        headroom,
                        &s,
                        &n,
                    )?;
                }
            }
        }
    }
    for (ni, noise) in noises.iter().enumerate() {
        for &gain_db in gains {
            let scaled = noise
                .iter()
                .map(|v| v * 10.0_f32.powf(gain_db / 20.0))
                .collect::<Vec<_>>();
            add_case(
                &mut suite,
                out,
                "noise",
                None,
                Some(ni),
                gain_db,
                None,
                1.0,
                &vec![0.0; noise.len()],
                &scaled,
            )?;
        }
    }
    save_json(&out.join("suite.json"), &suite)?;
    println!(
        "{} cases: {}",
        suite.cases.len(),
        out.join("suite.json").display()
    );
    Ok(())
}

fn load_suite(path: &Path) -> Result<Suite> {
    let suite: Suite = serde_json::from_slice(&fs::read(path)?)?;
    ensure!(
        suite.schema == 1 && suite.sample_rate == SAMPLE_RATE,
        "unsupported suite schema or sample rate"
    );
    ensure!(!suite.cases.is_empty(), "empty suite");
    let dir = path.parent().context("suite has no parent")?;
    let mut ids = std::collections::HashSet::new();
    for case in &suite.cases {
        ensure!(ids.insert(&case.id), "duplicate case ID");
        ensure!(
            case.id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
            "invalid case ID"
        );
        for a in [&case.input, &case.speech, &case.noise] {
            ensure!(
                Path::new(&a.file).components().count() == 1,
                "artifact must be a filename"
            );
            ensure!(
                audio::hash(&dir.join(&a.file))? == a.sha256,
                "{} has changed",
                a.file
            );
        }
        let s = audio::read(&dir.join(&case.speech.file))?;
        let n = audio::read(&dir.join(&case.noise.file))?;
        let x = audio::read(&dir.join(&case.input.file))?;
        ensure!(
            case.samples > 0 && s.len() == case.samples && n.len() == s.len() && x.len() == s.len(),
            "invalid case lengths"
        );
        ensure!(
            s.iter()
                .zip(n)
                .zip(x)
                .all(|((s, n), x)| (s + n - x).abs() < 1.0e-6),
            "components do not reconstruct input"
        );
    }
    Ok(suite)
}

fn process(
    net: &mut NoiseNet,
    input: &[f32],
    mode: Mode,
    times: &mut Vec<f64>,
) -> Result<Vec<f32>> {
    let mut delayed = Vec::with_capacity(input.len() + ALGORITHM_LATENCY_SAMPLES + FRAME_SIZE);
    for frame in 0..input.len().div_ceil(FRAME_SIZE) + ALGORITHM_LATENCY_SAMPLES / FRAME_SIZE {
        let start = frame * FRAME_SIZE;
        let mut x = [0.0; FRAME_SIZE];
        let mut y = [0.0; FRAME_SIZE];
        if start < input.len() {
            let end = (start + FRAME_SIZE).min(input.len());
            x[..end - start].copy_from_slice(&input[start..end]);
        }
        let begin = Instant::now();
        match mode {
            Mode::Product => net.process_frame(&x, &mut y),
            Mode::Reference => net.process_frame_unprotected(&x, &mut y),
            Mode::Passthrough => y = x,
        }
        times.push(begin.elapsed().as_secs_f64() * 1000.0);
        ensure!(
            y.iter().all(|v| v.is_finite()),
            "processor produced nonfinite output at frame {frame}"
        );
        delayed.extend(y);
    }
    let delay = if matches!(mode, Mode::Passthrough) {
        0
    } else {
        ALGORITHM_LATENCY_SAMPLES
    };
    Ok(delayed[delay..delay + input.len()].to_vec())
}

#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
fn timings(mut times: Vec<f64>) -> Timing {
    times.sort_by(f64::total_cmp);
    let n = times.len();
    if n == 0 {
        return Timing::default();
    }
    Timing {
        frames: n,
        mean_ms: times.iter().sum::<f64>() / n as f64,
        p95_ms: times[((n - 1) as f64 * 0.95).round() as usize],
        p99_ms: times[((n - 1) as f64 * 0.99).round() as usize],
        max_ms: times[n - 1],
        deadline_misses: times.iter().filter(|&&v| v > 10.0).count(),
    }
}

fn run(
    suite_path: &Path,
    out: &Path,
    mode: Mode,
    processor_id: &str,
    continuous: bool,
) -> Result<()> {
    let suite = load_suite(suite_path)?;
    let dir = suite_path.parent().context("suite has no parent")?;
    let processors = noise_net::processors();
    let processor = processors
        .iter()
        .find(|p| p.id() == processor_id)
        .context("processor unavailable")?;
    let mut net = NoiseNet::new(processor, processor.default_runtime())?;
    fs::create_dir(out).context("run directory must not already exist")?;
    let mut times = vec![];
    if continuous {
        let mut input = vec![];
        for case in &suite.cases {
            input.extend(audio::read(&dir.join(&case.input.file))?);
        }
        let output = process(&mut net, &input, mode, &mut times)?;
        let mut offset = 0;
        for case in &suite.cases {
            audio::write(
                &out.join(format!("{}.wav", case.id)),
                &output[offset..offset + case.samples],
            )?;
            offset += case.samples;
        }
    } else {
        for case in &suite.cases {
            net.reset();
            let input = audio::read(&dir.join(&case.input.file))?;
            let output = process(&mut net, &input, mode, &mut times)?;
            audio::write(&out.join(format!("{}.wav", case.id)), &output)?;
            eprintln!("{} {}", case.id, case.kind);
        }
    }
    let label = format!(
        "noise-net {} {mode:?}; {}; {}",
        env!("CARGO_PKG_VERSION"),
        processor.name(),
        processor.default_runtime()
    );
    let report = collect_scores(
        suite_path,
        &suite,
        out,
        label,
        Some(continuous),
        0,
        Some(timings(times)),
    )?;
    write_report(out, &report)
}

fn align(output: &[f32], delay: usize, samples: usize) -> Result<&[f32]> {
    let end = delay.checked_add(samples).context("delay overflow")?;
    ensure!(
        output.len() == end,
        "expected exactly {end} output samples including delay; got {}. Trim only documented padding before scoring",
        output.len()
    );
    Ok(&output[delay..end])
}

fn collect_scores(
    suite_path: &Path,
    suite: &Suite,
    recordings: &Path,
    label: String,
    continuous: Option<bool>,
    delay_samples: usize,
    timing: Option<Timing>,
) -> Result<Report> {
    let dir = suite_path.parent().context("suite has no parent")?;
    let mut rows = vec![];
    for case in &suite.cases {
        let s = audio::read(&dir.join(&case.speech.file))?;
        let n = audio::read(&dir.join(&case.noise.file))?;
        let x = audio::read(&dir.join(&case.input.file))?;
        let path = recordings.join(format!("{}.wav", case.id));
        let output = audio::read(&path)?;
        let y = align(&output, delay_samples, case.samples)?;
        rows.push(ResultRow {
            id: case.id.clone(),
            kind: case.kind.clone(),
            scores: metrics::score(&s, &n, &x, y),
            output_sha256: audio::hash(&path)?,
        });
    }
    Ok(Report {
        schema: 1,
        label,
        suite_sha256: audio::hash(suite_path)?,
        reference_kind: suite.reference_kind,
        continuous,
        delay_samples,
        timing,
        rows,
    })
}

fn write_report(dir: &Path, report: &Report) -> Result<()> {
    use std::fmt::Write;
    save_json(&dir.join("report.json"), report)?;
    let value = |v: Option<f64>| v.map_or_else(String::new, |v| format!("{v:.4}"));
    let mut csv = String::from(
        "id,kind,reference_sdr_db,input_sdr_db,output_attenuation_db,active_drop6_percent,longest_drop6_ms,speech_dominant_db,noise_dominant_db,band_2k_4k_db,band_4k_8k_db,output_peak\n",
    );
    for r in &report.rows {
        let s = &r.scores;
        writeln!(
            csv,
            "{},{},{},{},{:.4},{},{},{},{},{},{},{:.6}",
            r.id,
            r.kind,
            value(s.reference_sdr_db),
            value(s.input_sdr_db),
            s.output_attenuation_db,
            value(s.active_frames_drop6_percent),
            s.longest_active_drop6_ms,
            value(s.speech_dominant_attenuation_db),
            value(s.noise_dominant_attenuation_db),
            value(s.reference_band_attenuation_db[3]),
            value(s.reference_band_attenuation_db[4]),
            s.output_peak
        )?;
    }
    fs::write(dir.join("scores.csv"), csv)?;
    println!("{}", dir.join("report.json").display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::align;
    #[test]
    fn external_alignment_requires_exact_duration_and_declared_delay() {
        assert_eq!(align(&[0.0, 0.0, 0.2, 0.3], 2, 2).unwrap(), &[0.2, 0.3]);
        assert!(align(&[0.0, 0.0, 0.2], 2, 2).is_err());
        assert!(align(&[0.0, 0.0, 0.2, 0.3], 0, 2).is_err());
        assert!(align(&[0.0], usize::MAX, 1).is_err());
    }
}
