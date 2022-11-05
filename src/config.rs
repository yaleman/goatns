use clap::ArgMatches;
use config::{Config, File};
use flexi_logger::LoggerHandle;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::net::IpAddr;
use std::path::PathBuf;

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Default)]
pub struct IPAllowList {
    // Allow CH TXT VERSION.BIND or VERSION requests
    // pub version: Vec<IpAddr>,
    /// A list of allowed IPs to send a shutdown record from
    pub shutdown: Vec<IpAddr>,
}

#[derive(Deserialize, Debug, Eq, PartialEq, Clone, Serialize)]
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
    /// The location for the zone sqlite file
    pub sqlite_path: String,
    /// Where the JSON zone file is
    pub zone_file: Option<String>,
    /// IP Allow lists
    pub ip_allow_lists: IPAllowList,
    /// API / Web UI Port
    pub api_port: u16,
    /// Certificate path
    pub api_tls_cert: PathBuf,
    /// TLS key path
    pub api_tls_key: PathBuf,
    /// Static File Directory for api things
    pub api_static_dir: PathBuf,
}

impl ConfigFile {
    /// get a string version of the listener address
    pub fn api_listener_address(&self) -> String {
        format!("{}:{}", self.address, self.api_port)
    }
}

impl Default for ConfigFile {
    fn default() -> Self {
        Self {
            address: "127.0.0.1".to_string(),
            port: 15353,
            capture_packets: false,
            log_level: "INFO".to_string(),
            tcp_client_timeout: 15,
            enable_hinfo: false,
            ip_allow_lists: IPAllowList {
                // version: vec![],
                shutdown: vec![],
            },
            sqlite_path: String::from("~/.cache/goatns.sqlite"),
            zone_file: None,
            api_port: 9000,
            api_tls_cert: PathBuf::from("./certificates/cert.pem"),
            api_tls_key: PathBuf::from("./certificates/key.pem"),
            api_static_dir: PathBuf::from("./static_files/"),
        }
    }
}

impl Display for ConfigFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "listening_address=\"{}:{}\" api_endpoint=\"https://{}\" capturing_pcaps={} Log_level={}",
            self.address, self.port, self.api_listener_address(), self.capture_packets, self.log_level
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
            ip_allow_lists: config
                .get("ip_allow_lists")
                .unwrap_or(Self::default().ip_allow_lists),
            tcp_client_timeout: config
                .get("tcp_client_timeout")
                .unwrap_or(Self::default().tcp_client_timeout),
            sqlite_path: config
                .get("sqlite_path")
                .unwrap_or(Self::default().sqlite_path),
            zone_file: config.get("zone_file").unwrap_or(Self::default().zone_file),
            api_port: config.get("api_port").unwrap_or(Self::default().api_port),
            api_tls_cert: config
                .get("api_tls_cert")
                .unwrap_or(Self::default().api_tls_cert),
            api_tls_key: config
                .get("api_tls_key")
                .unwrap_or(Self::default().api_tls_key),
            api_static_dir: config
                .get("api_static_dir")
                .unwrap_or(Self::default().api_static_dir),
        }
    }
}

lazy_static! {
    static ref CONFIG_LOCATIONS: Vec<&'static str> =
        ["./goatns.json", "~/.config/goatns.json",].to_vec();
}

/// Loads the configuration from a given file or from some default locations.
///
/// The default locations are `~/.config/goatns.json` and `./goatns.json`.
pub fn get_config(config_path: Option<&String>) -> ConfigFile {
    let file_locations = match config_path {
        Some(value) => vec![value.to_owned()],
        None => CONFIG_LOCATIONS.iter().map(|x| x.to_string()).collect(),
    };

    for filepath in file_locations {
        let config_filename: String = shellexpand::tilde(&filepath).into_owned();
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

pub fn check_config(config: &ConfigFile) -> Result<(), Vec<String>> {
    let mut config_ok: bool = true;
    let mut errors: Vec<String> = vec![];
    if !config.api_tls_key.exists() {
        errors.push(format!(
            "Failed to find API TLS Key file: {:?}",
            config.api_tls_key
        ));
        config_ok = false;
    };

    if !config.api_tls_cert.exists() {
        errors.push(format!(
            "Failed to find API TLS cert file: {:?}",
            config.api_tls_cert
        ));
        config_ok = false;
    };

    match config_ok {
        true => Ok(()),
        false => Err(errors),
    }
}

pub fn setup_logging(
    config: &ConfigFile,
    clap_results: &ArgMatches,
) -> Result<LoggerHandle, String> {
    // force the log level to info if we're testing config
    let log_level = match clap_results.get_flag("configcheck") {
        true => "info".to_string(),
        false => config.log_level.to_ascii_lowercase(),
    };

    match flexi_logger::Logger::try_with_str(&log_level).map_err(|e| format!("{e:?}")) {
        Ok(logger) => logger
            .write_mode(flexi_logger::WriteMode::Async)
            .set_palette("b1;3;2;6;5".to_string())
            .start()
            .map_err(|e| format!("{e:?}")),
        Err(error) => Err(format!("Failed to start logger! {error:?}")),
    }
}
