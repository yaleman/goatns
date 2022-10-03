use std::fmt::Display;

use config::{Config, File};
use serde::Deserialize;

#[derive(Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct ConfigFile {
    pub address: String,
    pub port: u16,
    pub capture_packets: bool,
    pub log_level: String,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            address: "0.0.0.0".to_string(),
            port: 15353,
            capture_packets: false,
            log_level: "DEBUG".to_string(),
        }
    }
}

impl Display for ConfigFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "Listening address=\"{}:{}\" Capturing={} Log level={}",
            self.address, self.port, self.capture_packets, self.log_level
        ))
    }
}

impl From<Config> for ConfigFile {
    fn from(config: Config) -> Self {
        ConfigFile {
            address: config.get("addr").unwrap_or(Self::default().address),
            port: config.get("port").unwrap_or_default(),
            capture_packets: config.get("capture_packets").unwrap_or_default(),
            log_level: config.get("log_level").unwrap_or(Self::default().log_level),
        }
    }
}

pub fn get_config() -> ConfigFile {
    for filepath in ["~/.config/goatns.json", "goatns.json"] {
        let config_file = String::from(filepath);
        let config_filename: String = shellexpand::tilde(&config_file).into_owned();
        let config_filepath = std::path::Path::new(&config_filename);
        match config_filepath.exists() {
            false => {
                eprintln!("Config file {} doesn't exist, skipping.", config_filename)
            }
            true => {
                let builder = Config::builder()
                    .add_source(File::new(&config_filename, config::FileFormat::Json))
                    .add_source(config::Environment::with_prefix("goatns"));

                match builder.build() {
                    Ok(config) => {
                        println!("Successfully loaded config from: {}", config_filename);
                        return config.into();
                    }
                    Err(error) => {
                        eprintln!("Couldn't load config from {}: {:?}", config_filename, error);
                    }
                }
            }
        }
    }
    ConfigFile::default()
}
