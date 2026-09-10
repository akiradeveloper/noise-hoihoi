use std::{collections::HashMap, io::Write as _, path::Path};

use directories::ProjectDirs;
use noise_hoihoi_session::Settings;

pub struct SettingsFile;

impl SettingsFile {
    pub fn load() -> (Settings, Option<String>) {
        let Some(dirs) = ProjectDirs::from("", "", "NoiseHoiHoi") else {
            return (
                Settings::default(),
                Some("Settings directory is unavailable.".into()),
            );
        };
        match Self::load_from(dirs.data_dir()) {
            Ok(settings) => (settings, None),
            Err(error) => (
                Settings::default(),
                Some(format!("Could not load settings: {error}")),
            ),
        }
    }

    fn load_from(directory: &Path) -> Result<Settings, Box<dyn std::error::Error>> {
        let json = directory.join("settings.json");
        if json.exists() {
            return Ok(serde_json::from_slice(&std::fs::read(json)?)?);
        }
        // eframe stored a RON-encoded settings value inside its RON key/value map.
        let legacy = directory.join("app.ron");
        if legacy.exists() {
            let values: HashMap<String, String> = ron::from_str(&std::fs::read_to_string(legacy)?)?;
            if let Some(value) = values.get("noise-hoihoi-settings") {
                return Ok(ron::from_str(value)?);
            }
        }
        Ok(Settings::default())
    }

    pub fn save(settings: &Settings) -> Result<(), Box<dyn std::error::Error>> {
        let dirs =
            ProjectDirs::from("", "", "NoiseHoiHoi").ok_or("Settings directory is unavailable")?;
        Self::save_to(settings, dirs.data_dir())
    }

    fn save_to(settings: &Settings, directory: &Path) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(directory)?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        temporary.write_all(&serde_json::to_vec_pretty(settings)?)?;
        temporary.as_file().sync_all()?;
        temporary.persist(directory.join("settings.json"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_egui_settings_and_prefers_subsequent_gpui_settings() {
        let directory = tempfile::tempdir().unwrap();
        let mut settings = Settings {
            input_device_id: Some("microphone-id".into()),
            processor_id: Some("gpu-id".into()),
            noise_reduction: true,
        };
        let values = HashMap::from([("noise-hoihoi-settings", ron::to_string(&settings).unwrap())]);
        let legacy = ron::to_string(&values).unwrap();
        std::fs::write(directory.path().join("app.ron"), &legacy).unwrap();
        assert_eq!(SettingsFile::load_from(directory.path()).unwrap(), settings);
        settings.noise_reduction = false;
        SettingsFile::save_to(&settings, directory.path()).unwrap();
        assert_eq!(SettingsFile::load_from(directory.path()).unwrap(), settings);
        settings.processor_id = None;
        SettingsFile::save_to(&settings, directory.path()).unwrap();
        assert_eq!(SettingsFile::load_from(directory.path()).unwrap(), settings);
        assert_eq!(
            std::fs::read_to_string(directory.path().join("app.ron")).unwrap(),
            legacy
        );
    }

    #[test]
    fn malformed_settings_are_reported() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("settings.json"), "invalid").unwrap();
        assert!(SettingsFile::load_from(directory.path()).is_err());
    }
}
