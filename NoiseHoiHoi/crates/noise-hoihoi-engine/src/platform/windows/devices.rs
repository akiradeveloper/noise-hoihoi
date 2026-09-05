use cpal::{
    Device, SampleFormat, SupportedStreamConfig,
    traits::{DeviceTrait as _, HostTrait as _},
};

use crate::{
    AudioDevice, EngineError, PIPELINE_SAMPLE_RATE, VB_CABLE_PLAYBACK_ENDPOINT_NAME,
    is_vb_cable_playback_endpoint, is_vb_cable_recording_endpoint,
};

/// Enumerate physical capture endpoints, excluding VB-CABLE's recording side
/// to prevent accidental feedback loops.
///
/// # Errors
///
/// Returns an error when WASAPI cannot enumerate the capture endpoints.
pub fn input_devices() -> Result<Vec<AudioDevice>, EngineError> {
    let host = cpal::default_host();
    let default_id = host
        .default_input_device()
        .and_then(|device| device.id().ok())
        .map(|id| id.to_string());
    let devices = host
        .input_devices()
        .map_err(|error| EngineError::DeviceEnumeration(error.to_string()))?;

    let mut result = Vec::new();
    for device in devices {
        let (Ok(id), Ok(description)) = (device.id(), device.description()) else {
            continue;
        };
        if is_vb_cable_recording_endpoint(description.name()) {
            continue;
        }
        let id = id.to_string();
        result.push(AudioDevice {
            is_default: default_id.as_deref() == Some(id.as_str()),
            id,
            name: description.name().to_owned(),
        });
    }

    result.sort_by(|left, right| {
        right
            .is_default
            .cmp(&left.is_default)
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
    });
    Ok(result)
}

pub(super) fn find_input_device(host: &cpal::Host, wanted_id: &str) -> Result<Device, EngineError> {
    let devices = host
        .input_devices()
        .map_err(|error| EngineError::DeviceEnumeration(error.to_string()))?;
    for device in devices {
        if device.id().is_ok_and(|id| id.to_string() == wanted_id) {
            return Ok(device);
        }
    }
    Err(EngineError::InputDeviceNotFound(wanted_id.to_owned()))
}

pub(super) fn find_vb_cable_playback_device(host: &cpal::Host) -> Result<Device, EngineError> {
    let devices = host
        .output_devices()
        .map_err(|error| EngineError::DeviceEnumeration(error.to_string()))?;
    for device in devices {
        if device
            .description()
            .is_ok_and(|description| is_vb_cable_playback_endpoint(description.name()))
        {
            return Ok(device);
        }
    }
    Err(EngineError::VbCablePlaybackNotFound)
}

pub(super) fn default_input_format(device: &Device) -> Result<SupportedStreamConfig, EngineError> {
    let format = device
        .default_input_config()
        .map_err(|error| EngineError::UnsupportedFormat(error.to_string()))?;
    reject_dsd(format.sample_format(), "input")?;
    Ok(format)
}

pub(super) fn vb_cable_output_format(
    device: &Device,
) -> Result<SupportedStreamConfig, EngineError> {
    let configs = device
        .supported_output_configs()
        .map_err(|error| EngineError::UnsupportedFormat(error.to_string()))?;
    configs
        .filter(|range| {
            range.contains_rate(PIPELINE_SAMPLE_RATE) && !range.sample_format().is_dsd()
        })
        .min_by_key(|range| sample_format_rank(range.sample_format()))
        .map(|range| range.with_sample_rate(PIPELINE_SAMPLE_RATE))
        .ok_or_else(|| {
            EngineError::UnsupportedFormat(format!(
                "{VB_CABLE_PLAYBACK_ENDPOINT_NAME} must support 48 kHz PCM"
            ))
        })
}

fn sample_format_rank(format: SampleFormat) -> u8 {
    match format {
        SampleFormat::F32 => 0,
        SampleFormat::I16 => 1,
        SampleFormat::I24 => 2,
        SampleFormat::I32 => 3,
        SampleFormat::F64 => 4,
        _ if format.is_dsd() => u8::MAX,
        _ => 5,
    }
}

fn reject_dsd(format: SampleFormat, endpoint: &str) -> Result<(), EngineError> {
    if format.is_dsd() {
        Err(EngineError::UnsupportedFormat(format!(
            "{endpoint} uses unsupported DSD format {format}"
        )))
    } else {
        Ok(())
    }
}
