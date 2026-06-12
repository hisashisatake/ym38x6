use configparser::ini::Ini;
use std::path::PathBuf;

fn settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("settings.ini")))
        .unwrap_or_else(|| PathBuf::from("settings.ini"))
}

pub struct Settings {
    /// "wms1" または "ym38x6"。main.rsの`EngineHandle`で使用するエンジンを切り替える。
    pub engine_type: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self { engine_type: "wms1".to_string() }
    }
}

impl Settings {
    pub fn load() -> Self {
        let path = settings_path();
        let mut config = Ini::new();
        if config.load(&path).is_err() {
            eprintln!("settings.ini not found at {:?}, using defaults", path);
            return Self::default();
        }
        let engine_type = config
            .get("engine", "type")
            .unwrap_or_else(|| "wms1".to_string());
        eprintln!("settings loaded: engine={}", engine_type);
        Self { engine_type }
    }
}
