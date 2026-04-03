use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use directories::ProjectDirs;
use tracing::{debug, error};

use crate::content::{SortOrder, SortType};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub sort_files: SortType,
    pub sort_order: SortOrder,
    pub max_load_use_memory: usize,
    pub last_open_dir: Option<PathBuf>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            sort_files: SortType::FileName,
            sort_order: SortOrder::Ascending,
            max_load_use_memory: 500 * 1024 * 1024, // 500MB
            last_open_dir: directories::UserDirs::new().and_then(|ud| ud.picture_dir().map(|p| p.to_path_buf())),
        }
    }
}

impl AppSettings {
    pub fn get_config_dir() -> Option<PathBuf> {
        ProjectDirs::from("", "", "rimze").map(|dirs| dirs.config_dir().to_path_buf())
    }

    pub fn get_config_path() -> Option<PathBuf> {
        Self::get_config_dir().map(|dir| dir.join("settings.yaml"))
    }

    pub fn load() -> Self {
        if let Some(path) = Self::get_config_path() {
            if path.exists() {
                match std::fs::read_to_string(&path) {
                    Ok(content) => {
                        match serde_yaml::from_str(&content) {
                            Ok(settings) => {
                                debug!("Loaded settings from {:?}", path);
                                return settings;
                            }
                            Err(e) => error!("Failed to deserialize settings: {}", e),
                        }
                    }
                    Err(e) => error!("Failed to read settings file: {}", e),
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        if let Some(path) = Self::get_config_path() {
            if let Some(dir) = path.parent() {
                if let Err(e) = std::fs::create_dir_all(dir) {
                    error!("Failed to create config directory: {}", e);
                    return;
                }
            }
            match serde_yaml::to_string(self) {
                Ok(content) => {
                    if let Err(e) = std::fs::write(&path, content) {
                        error!("Failed to write settings file: {}", e);
                    } else {
                        debug!("Saved settings to {:?}", path);
                    }
                }
                Err(e) => error!("Failed to serialize settings: {}", e),
            }
        }
    }
}
