mod devices;
mod streams;
use crate::{AudioProcessor, EngineConfig, EngineError, EngineState, RunningAudioEngine};
use cpal::{Stream, traits::StreamTrait as _};
pub use devices::input_devices;
use std::sync::Arc;
use streams::{build_input_stream, build_output_stream};

// Fields drop in declaration order, before the common owner joins its worker.
struct WindowsRoute {
    _input: Stream,
    _output: Stream,
}

/// Start physical microphone -> processor -> VB-CABLE playback endpoint.
///
/// # Errors
///
/// Returns an error for an invalid selection, unsupported endpoint format, a
/// missing virtual endpoint, or a stream/worker startup failure.
pub fn start<P: AudioProcessor>(
    config: &EngineConfig,
    processor: P,
) -> Result<RunningAudioEngine, EngineError> {
    config.validate()?;

    let host = cpal::default_host();
    let input = devices::find_input_device(&host, &config.input_device_id)?;
    let output = devices::find_vb_cable_playback_device(&host)?;
    let input_format = devices::default_input_format(&input)?;
    let output_format = devices::vb_cable_output_format(&output)?;

    let input_rate = input_format.sample_rate();
    let input_channels = usize::from(input_format.channels());
    let output_channels = usize::from(output_format.channels());
    if input_rate == 0 || input_channels == 0 || output_channels == 0 {
        return Err(EngineError::UnsupportedFormat(
            "an endpoint reported zero channels or sample rate".to_owned(),
        ));
    }

    let (engine, ports) = RunningAudioEngine::prepare(config, input_rate, processor)?;
    let shared_metrics = ports.metrics;
    let output_consumer = ports.output;
    let input_producer = ports.input;
    let worker_thread = ports.worker;

    let output_stream = build_output_stream(
        &output,
        &output_format,
        output_channels,
        output_consumer,
        Arc::clone(&shared_metrics),
    )?;
    let input_stream = build_input_stream(
        &input,
        &input_format,
        input_channels,
        input_producer,
        worker_thread,
        Arc::clone(&shared_metrics),
    )?;

    if let Err(error) = output_stream.play() {
        drop(input_stream);
        drop(output_stream);
        return Err(EngineError::Start(format!(
            "failed to start VB-CABLE output: {error}"
        )));
    }
    if let Err(error) = input_stream.play() {
        drop(input_stream);
        drop(output_stream);
        return Err(EngineError::Start(format!(
            "failed to start microphone input: {error}"
        )));
    }

    shared_metrics.set_state(EngineState::Running);
    Ok(engine.with_route(WindowsRoute {
        _input: input_stream,
        _output: output_stream,
    }))
}
