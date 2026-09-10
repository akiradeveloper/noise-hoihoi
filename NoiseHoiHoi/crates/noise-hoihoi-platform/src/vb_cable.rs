/// VB-CABLE's playback endpoint, fed by the `NoiseHoiHoi` application.
pub const VB_CABLE_PLAYBACK_ENDPOINT_NAME: &str = "CABLE Input (VB-Audio Virtual Cable)";

/// VB-CABLE's recording endpoint, selected by OBS and other applications.
pub const VB_CABLE_RECORDING_ENDPOINT_NAME: &str = "CABLE Output (VB-Audio Virtual Cable)";

/// Returns whether a Windows endpoint name identifies VB-CABLE's playback side.
#[must_use]
pub fn is_vb_cable_playback_endpoint(name: &str) -> bool {
    endpoint_name_matches(name, "cable input", VB_CABLE_PLAYBACK_ENDPOINT_NAME)
}

/// Returns whether a Windows endpoint name identifies VB-CABLE's recording side.
#[must_use]
pub fn is_vb_cable_recording_endpoint(name: &str) -> bool {
    endpoint_name_matches(name, "cable output", VB_CABLE_RECORDING_ENDPOINT_NAME)
}

fn endpoint_name_matches(actual: &str, short_name: &str, full_name: &str) -> bool {
    let actual = actual.to_ascii_lowercase();
    actual == full_name.to_ascii_lowercase()
        || actual == short_name
        || (actual.starts_with(short_name) && actual.contains("vb-audio virtual cable"))
}

#[cfg(test)]
mod tests {
    use super::{is_vb_cable_playback_endpoint, is_vb_cable_recording_endpoint};

    #[test]
    fn accepts_full_and_short_playback_names_case_insensitively() {
        assert!(is_vb_cable_playback_endpoint(
            "CABLE Input (VB-Audio Virtual Cable)"
        ));
        assert!(is_vb_cable_playback_endpoint("cable input"));
        assert!(is_vb_cable_playback_endpoint(
            "CABLE INPUT (VB-AUDIO VIRTUAL CABLE)"
        ));
    }

    #[test]
    fn keeps_playback_and_recording_names_distinct() {
        assert!(is_vb_cable_recording_endpoint(
            "CABLE Output (VB-Audio Virtual Cable)"
        ));
        assert!(!is_vb_cable_playback_endpoint(
            "CABLE Output (VB-Audio Virtual Cable)"
        ));
        assert!(!is_vb_cable_recording_endpoint("unrelated microphone"));
    }
}
