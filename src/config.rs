use std::fmt::Display;
use std::net::IpAddr;

use config::{Config, File};
use serde::Deserialize;

#[derive(Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct ConfigFile {
    /// Listen address, default is 0.0.0.0
    pub address: String,
    /// Listen on this port, default is 15353
    pub port: u16,
    pub capture_packets: bool,
    /// Default is "DEBUG"
    pub log_level: String,
    /// How long until we drop client connections
    pub tcp_client_timeout: u16,
    /// Enable a HINFO record at hinfo.goat
    pub enable_hinfo: bool,
    /// A list of allowed IPs to send a shutdown record from
    pub shutdown_ip_allow_list: Vec<IpAddr>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            address: "0.0.0.0".to_string(),
            port: 15353,
            capture_packets: false,
            log_level: "DEBUG".to_string(),
            tcp_client_timeout: 15,
            enable_hinfo: false,
            shutdown_ip_allow_list: vec![],
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
            enable_hinfo: config
                .get("enable_hinfo")
                .unwrap_or(Self::default().enable_hinfo),
            shutdown_ip_allow_list: config
                .get("shutdown_ip_allow_list")
                .unwrap_or(Self::default().shutdown_ip_allow_list),
            tcp_client_timeout: config
                .get("tcp_client_timeout")
                .unwrap_or(Self::default().tcp_client_timeout),
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
