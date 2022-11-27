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
use std::io::ErrorKind;
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
/// The main config blob for GoatNS, write this as a JSON file and load it and it'll make things go.
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
    /// List of "valid" TLDs - if this is empty let anything be created
    pub allowed_tlds: Vec<String>,
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
    /// Secret for cookie storage - it'll randomly generate on startup by default
    #[serde(default = "generate_cookie_secret", skip_serializing)]
    api_cookie_secret: String,
    /// OAuth2 Resource server name
    pub oauth2_client_id: String,
    /// If your instance is behind a proxy/load balancer/whatever, you need to specify this, eg `https://example.com:12345`
    pub oauth2_redirect_url: Url,
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
    pub sql_db_cleanup_seconds: u64,
    /// Administrator contact details
    pub admin_contact: ContactDetails,
    /// Allow auto-provisioning of users
    pub user_auto_provisioning: bool,
}

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

    /// It's a sekret!
    pub fn api_cookie_secret(&self) -> &[u8] {
        self.api_cookie_secret.as_bytes()
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
        RustlsConfig::from_pem_file(self.api_tls_cert.clone(), self.api_tls_key.clone())
            .await
            .unwrap()
    }

    pub async fn check_config(
        mut config: CowCellWriteTxn<'_, ConfigFile>,
    ) -> Result<(), Vec<String>> {
        let mut errors: Vec<String> = vec![];

        if config.api_tls_cert.starts_with("~") {
            #[cfg(test)]
            eprintln!(
                "updating tls cert from {:#?} to shellex",
                config.api_tls_cert
            );
            config.api_tls_cert = PathBuf::from(
                shellexpand::tilde(&config.api_tls_cert.to_str().unwrap()).to_string(),
            );
        }
        if config.api_tls_key.starts_with("~") {
            #[cfg(test)]
            eprintln!("updating tls key from {:#?} to shellex", config.api_tls_key);
            config.api_tls_key = PathBuf::from(
                shellexpand::tilde(&config.api_tls_key.to_str().unwrap()).to_string(),
            );
        }

        if !config.api_tls_key.exists() {
            errors.push(format!(
                "Failed to find API TLS Key file: {:?}",
                config.api_tls_key
            ));
        };

        if !config.api_tls_cert.exists() {
            errors.push(format!(
                "Failed to find API TLS cert file: {:?}",
                config.api_tls_cert
            ));
        };

        config.commit().await;
        match errors.is_empty() {
            true => Ok(()),
            false => Err(errors),
        }
    }

    /// Uses [Self::try_from] and wraps it in a CowCell (moo)
    ///
    /// The default locations are `~/.config/goatns.json` and `./goatns.json`.
    pub fn try_as_cowcell(
        config_path: Option<&String>,
    ) -> Result<CowCell<ConfigFile>, std::io::Error> {
        Ok(CowCell::new(ConfigFile::try_from(config_path)?))
    }

    /// Loads the configuration from a given file or from some default locations.
    ///
    /// The default locations are `~/.config/goatns.json` and `./goatns.json`.
    pub fn try_from(config_path: Option<&String>) -> Result<ConfigFile, std::io::Error> {
        let file_locations = match config_path {
            Some(value) => vec![value.to_owned()],
            None => CONFIG_LOCATIONS.iter().map(|x| x.to_string()).collect(),
        };

        // clean up the file paths and filter them by the ones that exist
        let found_files: Vec<String> = file_locations
            .iter()
            .filter_map(|f| {
                let path = shellexpand::tilde(&f).into_owned();
                let filepath = std::path::Path::new(&path);
                match filepath.exists() {
                    false => {
                        eprintln!("Config file {path} doesn't exist, skipping.");
                        None
                    }
                    true => Some(path),
                }
            })
            .collect();

        if found_files.is_empty() {
            eprintln!(
                "No configuration files exist, giving up! Tried: {}",
                file_locations.join(", ")
            );
            return Err(std::io::Error::new(
                ErrorKind::NotFound,
                "No configuration files found",
            ));
        }

        // check that at least one config file exists
        for filepath in found_files {
            let config_filename: String = shellexpand::tilde(&filepath).into_owned();

            let builder = Config::builder()
                .add_source(File::new(&config_filename, config::FileFormat::Json))
                .add_source(config::Environment::with_prefix("goatns"));

            let config = builder.build().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Couldn't load config from {config_filename}: {e:?}"),
                )
            });

            match config {
                Ok(config) => {
                    eprintln!("Successfully loaded config from: {}", config_filename);
                    return Ok(ConfigFile::from(config));
                }
                Err(err) => eprintln!("{err:?}"),
            }
        }

        Ok(ConfigFile::default())
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
            allowed_tlds: vec![],
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
            // TODO: this should be auto-generated from stuff
            oauth2_redirect_url: Url::from_str("https://example.com")
                .expect("Internal error parsing example.com into a URL"),
            oauth2_secret: String::from(""),
            oauth2_config_url: String::from(""),
            oauth2_user_scopes: vec!["openid".to_string(), "email".to_string()],
            sql_log_slow_duration: 5,
            sql_log_statements: false,
            sql_db_cleanup_seconds: 3600, // one hour
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
            "hostname=\"{}\" listening_address=\"{}:{}\" capturing_pcaps={} Log_level={}, admin_contact={:?} {api_details} oauth2_redirect_url={}",
            self.hostname, self.address, self.port, self.capture_packets, self.log_level, self.admin_contact, self.oauth2_redirect_url
        ))
    }
}

impl From<Config> for ConfigFile {
    fn from(config: Config) -> Self {
        let hostname = config.get("hostname").unwrap_or(Self::default().hostname);
        let api_port = config.get("api_port").unwrap_or(Self::default().api_port);

        // The OAuth2 redirect URL is a magical pony.

        // TODO: test this with different values
        let oauth2_redirect_url: Option<Url> = match config.get("oauth2_redirect_url") {
            Ok(value) => {
                // println!("Found url in config: {:?}", value);
                Some(value)
            }
            Err(_) => None,
        };
        let oauth2_redirect_url = match oauth2_redirect_url {
            Some(mut url) => {
                // update the URL with the final auth path
                // eprintln!("OAuth2 Redirect URL: {url:?}");
                if !url.path().ends_with("/auth/login") {
                    // println!("Adding authlogin tail");
                    url = url.join("/auth/login").unwrap();
                }
                // eprintln!("OAuth2 Redirect URL after update: {url:?}");
                url
            }
            None => {
                // if they haven't set it in config, build it automagically
                let baseurl = match api_port {
                    443 => format!("https://{}", hostname),
                    _ => format!("https://{}:{}", hostname, api_port),
                };
                Url::parse(&format!("{}/auth/login", baseurl))
                    .expect("Failed to parse hostname/api_port into a valid URL")
            }
        };

        ConfigFile {
            hostname,
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
            allowed_tlds: config
                .get("allowed_tlds")
                .unwrap_or(Self::default().allowed_tlds),
            zone_file: config.get("zone_file").unwrap_or(Self::default().zone_file),
            enable_api: config
                .get("enable_api")
                .unwrap_or(Self::default().enable_api),
            api_port,
            api_tls_cert: config
                .get("api_tls_cert")
                .unwrap_or(Self::default().api_tls_cert),
            api_tls_key: config
                .get("api_tls_key")
                .unwrap_or(Self::default().api_tls_key),
            api_static_dir: config
                .get("api_static_dir")
                .unwrap_or(Self::default().api_static_dir),
            api_cookie_secret: config
                .get("api_cookie_secret")
                .unwrap_or(Self::default().api_cookie_secret),
            oauth2_client_id: config
                .get("oauth2_client_id")
                .unwrap_or(Self::default().oauth2_client_id),
            oauth2_redirect_url,
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
            sql_db_cleanup_seconds: config
                .get("sql_db_cleanup_seconds")
                .unwrap_or(Self::default().sql_db_cleanup_seconds),
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

pub async fn setup_logging(
    config: CowCellReadTxn<ConfigFile>,
    clap_results: &ArgMatches,
) -> Result<LoggerHandle, std::io::Error> {
    // force the log level to info if we're testing config
    let log_level = match clap_results.get_flag("configcheck") {
        true => "info".to_string(),
        false => config.log_level.to_ascii_lowercase(),
    };

    let logger = flexi_logger::Logger::try_with_str(log_level).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to start logger! {e:?}"),
        )
    })?;

    logger
        .write_mode(flexi_logger::WriteMode::Async)
        .filter(Box::new(LogFilter {
            filters: vec!["h2", "hyper::proto"],
        }))
        .set_palette("b1;3;2;6;5".to_string())
        .start()
        .map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to start logger! {e:?}"),
            )
        })
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
