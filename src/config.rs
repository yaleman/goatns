use clap::ArgMatches;
use config::{Config, File};
use flexi_logger::filter::{LogLineFilter, LogLineWriter};
use flexi_logger::{DeferredNow, LoggerHandle};
use gethostname::gethostname;
// use ipnet::IpNet;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;

/// Allow-listing ranges for making particular kinds of requests
#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Default)]
pub struct IPAllowList {
    // Allow CH TXT VERSION.BIND or VERSION requests
    // pub version: Vec<IpAddr>,
    /// IPs allowed to make AXFR requests
    // pub axfr: Vec<IpNet>,
    // TODO: Change shutdown from IpAddr to ipnet
    /// A list of allowed IPs which can send a "shutdown CH" request
    pub shutdown: Vec<IpAddr>,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Clone, Serialize)]
pub struct ConfigFile {
    /// The server's hostname when generating an SOA record, defaults to the results of gethostname()
    pub hostname: String,
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
    #[serde(flatten)]
    pub ip_allow_lists: IPAllowList,
    /// Do you really want an API?
    pub enable_api: bool,
    /// API / Web UI Port
    pub api_port: u16,
    /// Certificate path
    pub api_tls_cert: PathBuf,
    /// TLS key path
    pub api_tls_key: PathBuf,
    /// Static File Directory for api things
    pub api_static_dir: PathBuf,
    /// Secret for cookie storage - don't hard code this, it'll randomly generate on startup
    #[serde(default = "generate_cookie_secret", skip)]
    api_cookie_secret: String,
}

fn generate_cookie_secret() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 64)
}

impl ConfigFile {
    /// get a string version of the listener address
    pub fn api_listener_address(&self) -> SocketAddr {
        SocketAddr::from_str(&format!("{}:{}", self.address, self.api_port)).unwrap()
    }

    pub fn api_cookie_secret(self) -> String {
        self.api_cookie_secret
    }
}

impl Default for ConfigFile {
    fn default() -> Self {
        let hostname = gethostname();
        let hostname = hostname.into_string().unwrap();
        Self {
            hostname,
            address: "127.0.0.1".to_string(),
            port: 15353,
            capture_packets: false,
            log_level: "INFO".to_string(),
            tcp_client_timeout: 15,
            enable_hinfo: false,
            ip_allow_lists: IPAllowList {
                // axfr: vec![],
                shutdown: vec![],
            },
            sqlite_path: String::from("~/.cache/goatns.sqlite"),
            zone_file: None,
            enable_api: false,
            api_port: 9000,
            api_tls_cert: PathBuf::from("./certificates/cert.pem"),
            api_tls_key: PathBuf::from("./certificates/key.pem"),
            api_static_dir: PathBuf::from("./static_files/"),
            api_cookie_secret: generate_cookie_secret(),
        }
    }
}

impl Display for ConfigFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let api_details = match self.enable_api {
            false => format!("enable_api={}", self.enable_api),
            true => {
                format!(
                    "enable_api={}  api_endpoint=\"https://{}\" tls_cert={:?} tls_key={:?}",
                    self.enable_api,
                    self.api_listener_address(),
                    self.api_tls_cert,
                    self.api_tls_key
                )
            }
        };
        f.write_fmt(format_args!(
            "hostname=\"{}\" listening_address=\"{}:{}\" capturing_pcaps={} Log_level={}, {api_details}",
            self.hostname, self.address, self.port, self.capture_packets, self.log_level
        ))
    }
}

impl From<Config> for ConfigFile {
    fn from(config: Config) -> Self {
        ConfigFile {
            hostname: config.get("hostname").unwrap_or(Self::default().hostname),
            address: config.get("address").unwrap_or(Self::default().address),
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
            enable_api: config
                .get("enable_api")
                .unwrap_or(Self::default().enable_api),
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
            api_cookie_secret: generate_cookie_secret(),
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

pub fn check_config(config: &mut ConfigFile) -> Result<ConfigFile, Vec<String>> {
    let mut config_ok: bool = true;
    let mut errors: Vec<String> = vec![];

    if config.api_tls_cert.starts_with("~") {
        config.api_tls_cert =
            PathBuf::from(shellexpand::tilde(&config.api_tls_cert.to_str().unwrap()).to_string());
    }
    if config.api_tls_key.starts_with("~") {
        config.api_tls_key =
            PathBuf::from(shellexpand::tilde(&config.api_tls_key.to_str().unwrap()).to_string());
    }

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
        true => Ok(config.to_owned()),
        false => Err(errors),
    }
}

pub struct LogFilter {
    filters: Vec<&'static str>,
}

impl LogLineFilter for LogFilter {
    fn write(
        &self,
        now: &mut DeferredNow,
        record: &log::Record,
        log_line_writer: &dyn LogLineWriter,
    ) -> std::io::Result<()> {
        // eprintln!("{:?}", record.metadata());
        if self
            .filters
            .iter()
            .any(|r| record.metadata().target().starts_with(r))
        {
            return Ok(());
        }
        log_line_writer.write(now, record)?;
        Ok(())
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
            .filter(Box::new(LogFilter {
                filters: vec!["h2", "hyper::proto"],
            }))
            .set_palette("b1;3;2;6;5".to_string())
            .start()
            .map_err(|e| format!("{e:?}")),
        Err(error) => Err(format!("Failed to start logger! {error:?}")),
    }
}
