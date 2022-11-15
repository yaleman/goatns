use axum_server::tls_rustls::RustlsConfig;
use clap::ArgMatches;
use concread::cowcell::asynch::{CowCell, CowCellReadTxn, CowCellWriteTxn};
use config::{Config, File};
use flexi_logger::filter::{LogLineFilter, LogLineWriter};
use flexi_logger::{DeferredNow, LoggerHandle};
use gethostname::gethostname;
use rand::distributions::{Alphanumeric, DistString};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::net::IpAddr;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;
use url::Url;

use crate::enums::ContactDetails;

#[derive(Deserialize, Serialize, Debug, Eq, PartialEq, Clone, Default)]
/// Allow-listing ranges for making particular kinds of requests
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
    /// DNS listener address, default is 127.0.0.1
    pub address: String,
    /// Listen for DNS queries on this port, default is 15353
    pub port: u16,
    /// If we should capture packets on request/response
    pub capture_packets: bool,
    /// Default is "DEBUG"
    pub log_level: String,
    /// How long until we drop TCP client connections, defaults to 5 seconds.
    pub tcp_client_timeout: u64,
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
    pub api_static_dir: String,
    /// Secret for cookie storage - don't hard code this, it'll randomly generate on startup
    #[serde(default = "generate_cookie_secret", skip)]
    pub api_cookie_secret: String,
    /// OAuth2 Resource server name
    pub oauth2_client_id: String,
    /// If your instance is behind a proxy/load balancer/whatever, you need to specify this, eg `https://example.com:12345`
    pub oauth2_redirect_url: Option<Url>,
    #[serde(skip_serializing)]
    /// Oauth2 Secret
    pub oauth2_secret: String,
    /// OIDC Discovery URL, eg for Kanidm you'd use `https://idm.example.com/oauth2/openid/:client_id:/.well-known/openid-configuration`
    #[serde(default)]
    pub oauth2_config_url: String,
    #[serde(default)]
    /// A list of scopes to request from the IdP
    pub oauth2_user_scopes: Vec<String>,
    /// Log things sometimes
    pub sql_log_statements: bool,
    /// When queries take more than this many seconds, log them
    pub sql_log_slow_duration: u64,
    /// Clean up sessions table every n seconds
    pub sql_session_cleanup_seconds: u64,
    /// Administrator contact details
    pub admin_contact: ContactDetails,
    /// Allow auto-provisioning of users
    pub user_auto_provisioning: bool,
}

// impl From<&CowCellReadTxn<ConfigFile>> for ConfigFile {
//     fn from(input: &CowCellReadTxn<ConfigFile>) -> Self {
//         ConfigFile {
//             hostname: input.hostname.clone(),
//             address: input.address.clone(),
//             port: input.port,
//             capture_packets: input.capture_packets,
//             log_level: input.log_level.clone(),
//             tcp_client_timeout: input.tcp_client_timeout,
//             enable_hinfo: input.enable_hinfo,
//             sqlite_path: input.sqlite_path.clone(),
//             zone_file: input.zone_file.clone(),
//             ip_allow_lists: input.ip_allow_lists.clone(),
//             enable_api: input.enable_api,
//             api_port: input.api_port,
//             api_tls_cert: input.api_tls_cert.clone(),
//             api_tls_key: input.api_tls_key.clone(),
//             api_static_dir: input.api_static_dir.clone(),
//             api_cookie_secret: input.api_cookie_secret.clone(),
//             oauth2_client_id: input.oauth2_client_id.clone(),
//             oauth2_redirect_url: input.oauth2_redirect_url.clone(),
//             oauth2_secret: input.oauth2_secret.clone(),
//             oauth2_config_url: input.oauth2_config_url.clone(),
//             oauth2_user_scopes: input.oauth2_user_scopes.clone(),
//             sql_log_statements: input.sql_log_statements.clone(),
//             sql_log_slow_duration: input.sql_log_slow_duration.clone(),
//             sql_session_cleanup_seconds: input.sql_session_cleanup_seconds,
//             admin_contact: input.admin_contact.clone(),
//             user_auto_provisioning: input.user_auto_provisioning,
//         }
//     }
// }

fn generate_cookie_secret() -> String {
    Alphanumeric.sample_string(&mut rand::thread_rng(), 64)
}

impl ConfigFile {
    /// JSONify the configfile in a pretty way using serde
    pub fn as_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {e:?}"))
            .unwrap()
    }

    /// Get a bindable SocketAddr for use in the DNS listeners
    pub fn dns_listener_address(&self) -> Result<SocketAddr, Option<String>> {
        let listen_addr = format!("{}:{}", &self.address, &self.port);

        listen_addr.parse::<SocketAddr>().map_err(|e| {
            log::error!("Failed to parse address: {e:?}");
            None
        })
    }

    /// get a string version of the listener address
    pub fn api_listener_address(&self) -> SocketAddr {
        SocketAddr::from_str(&format!("{}:{}", self.address, self.api_port)).unwrap()
    }

    pub fn api_cookie_secret(self) -> String {
        self.api_cookie_secret
    }

    pub fn status_url(&self) -> Url {
        Url::from_str(&format!(
            "https://{}:{}/status",
            self.hostname, self.api_port
        ))
        .unwrap()
    }

    pub async fn get_tls_config(&self) -> RustlsConfig {
        log::trace!(
            "tls config: cert={:?} key={:?}",
            self.api_tls_cert,
            self.api_tls_key
        );
        RustlsConfig::from_pem_file(self.api_tls_cert.to_owned(), self.api_tls_key.to_owned())
            .await
            .unwrap()
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
            tcp_client_timeout: 5,
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
            api_static_dir: String::from("./static_files/"),
            api_cookie_secret: generate_cookie_secret(),
            oauth2_client_id: String::from(""),
            oauth2_redirect_url: None,
            oauth2_secret: String::from(""),
            oauth2_config_url: String::from(""),
            oauth2_user_scopes: vec!["openid".to_string(), "email".to_string()],
            sql_log_slow_duration: 5,
            sql_log_statements: false,
            sql_session_cleanup_seconds: 3600, // one hour
            admin_contact: Default::default(),
            user_auto_provisioning: false,
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
            "hostname=\"{}\" listening_address=\"{}:{}\" capturing_pcaps={} Log_level={}, admin_contact={:?} {api_details}",
            self.hostname, self.address, self.port, self.capture_packets, self.log_level, self.admin_contact,
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
            oauth2_client_id: config
                .get("oauth2_client_id")
                .unwrap_or(Self::default().oauth2_client_id),
            oauth2_redirect_url: config
                .get("oauth2_redirect_url")
                .unwrap_or(Self::default().oauth2_redirect_url),
            oauth2_secret: config
                .get("oauth2_secret")
                .unwrap_or(Self::default().oauth2_secret),
            oauth2_config_url: config
                .get("oauth2_config_url")
                .unwrap_or(Self::default().oauth2_config_url),
            oauth2_user_scopes: config
                .get("oauth2_user_scopes")
                .unwrap_or(Self::default().oauth2_user_scopes),
            sql_log_slow_duration: config
                .get("sql_log_slow_duration")
                .unwrap_or(Self::default().sql_log_slow_duration),
            sql_log_statements: config
                .get("sql_log_statements")
                .unwrap_or(Self::default().sql_log_statements),
            sql_session_cleanup_seconds: config
                .get("sql_session_cleanup_seconds")
                .unwrap_or(Self::default().sql_session_cleanup_seconds),
            admin_contact: config
                .get("admin_contact")
                .unwrap_or(Self::default().admin_contact),
            user_auto_provisioning: config
                .get("user_auto_provisioning")
                .unwrap_or(Self::default().user_auto_provisioning),
        }
    }
}

impl FromStr for ConfigFile {
    type Err = String;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        // let res =Config::try_from(&input);

        let configfile = File::from_str(input, config::FileFormat::Json);

        let res = Config::builder()
            .add_source(configfile)
            .build()
            .map_err(|e| format!("{e:?}"))?;

        let res: ConfigFile = res.into();
        Ok(res)
    }
}

lazy_static! {
    static ref CONFIG_LOCATIONS: Vec<&'static str> =
        ["./goatns.json", "~/.config/goatns.json",].to_vec();
}

/// Loads the configuration from a given file or from some default locations.
///
/// The default locations are `~/.config/goatns.json` and `./goatns.json`.
pub fn get_config_cowcell(config_path: Option<&String>) -> Result<CowCell<ConfigFile>, String> {
    match get_config(config_path) {
        Ok(value) => Ok(CowCell::new(value)),
        Err(err) => Err(err),
    }
}

pub fn get_config(config_path: Option<&String>) -> Result<ConfigFile, String> {
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
                        eprintln!("Successfully loaded config from: {}", config_filename);
                        return Ok(ConfigFile::from(config));
                    }
                    Err(error) => {
                        let err =
                            format!("Couldn't load config from {}: {:?}", config_filename, error);
                        log::error!("{err}");
                        return Err(err);
                    }
                }
            }
        }
    }

    Ok(ConfigFile::default())
}

pub async fn check_config(mut config: CowCellWriteTxn<'_, ConfigFile>) -> Result<(), Vec<String>> {
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

    config.commit().await;
    match config_ok {
        true => Ok(()),
        false => Err(errors),
    }
}

pub async fn setup_logging(
    config: CowCellReadTxn<ConfigFile>,
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
