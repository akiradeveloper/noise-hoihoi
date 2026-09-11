//! Native end-to-end benchmark: raw mono 48 kHz f32le input/output, optional pacing.
use anyhow::{Result, ensure};
use noise_net_iree::{DpdfNet, FRAME_SIZE};
use std::{
    io::Write,
    time::{Duration, Instant},
};
#[allow(clippy::cast_precision_loss)]
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    ensure!(
        args.len() >= 3,
        "stream INPUT.f32le OUTPUT.f32le [paced] [PROCESSOR_ID]"
    );
    let uri = args.get(4).map_or("cpu", String::as_str);
    eprintln!("Processor: {uri}");
    let bytes = std::fs::read(&args[1])?;
    ensure!(
        bytes.len() % (4 * FRAME_SIZE) == 0 && bytes.len() > 4 * FRAME_SIZE,
        "input must contain whole 480-sample hops"
    );
    let input: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().expect("f32")))
        .collect();
    ensure!(input.iter().all(|v| v.is_finite()), "non-finite input");
    let mut net = DpdfNet::new(uri)?;
    let paced = args.get(3).is_some_and(|s| s == "paced");
    let mut output = Vec::with_capacity(input.len());
    let mut ms = Vec::with_capacity(input.len() / FRAME_SIZE);
    let mut missed = 0;
    let start = Instant::now();
    let mut arrival = start;
    for (index, frame) in input.chunks_exact(FRAME_SIZE).enumerate() {
        if paced {
            std::thread::sleep(arrival.saturating_duration_since(Instant::now()));
        }
        let deadline = arrival + Duration::from_millis(10);
        let begin = Instant::now();
        let mut enhanced = [0.0; FRAME_SIZE];
        net.process_frame(frame.try_into()?, &mut enhanced)?;
        let end = Instant::now();
        if index > 0 {
            ms.push(end.duration_since(begin).as_secs_f64() * 1000.0);
        }
        if paced && end > deadline {
            missed += 1;
        }
        output.extend(enhanced);
        arrival = deadline;
    }
    let mut file = std::io::BufWriter::new(std::fs::File::create(&args[2])?);
    for sample in output {
        file.write_all(&sample.to_le_bytes())?;
    }
    file.flush()?;
    ms.sort_by(f64::total_cmp);
    let count = ms.len();
    let mean = ms.iter().sum::<f64>() / count as f64;
    println!(
        "{{\"count\":{count},\"mean_ms\":{mean},\"p95_ms\":{},\"p99_ms\":{},\"max_ms\":{},\"over_10ms\":{},\"deadline_misses\":{missed},\"paced\":{paced}}}",
        ms[count * 95 / 100],
        ms[count * 99 / 100],
        ms[count - 1],
        ms.iter().filter(|v| **v > 10.0).count()
    );
    Ok(())
}
