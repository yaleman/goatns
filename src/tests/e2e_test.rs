#[cfg(test)]
mod tests {
    use log::info;
    use std::env;
    use std::net::*;
    use std::thread::sleep;
    use trust_dns_resolver::config::*;
    use trust_dns_resolver::Resolver;

    fn in_github_actions() -> bool {
        env::var("GITHUB_ACTIONS").is_ok()
    }

    #[test]
    fn test_full_run() -> Result<(), std::io::Error> {
        if in_github_actions() {
            eprintln!("Skipping this test because it won't work in GHA");
            return Ok(());
        }

        // YOLO some certs
        std::process::Command::new("./insecure_generate_tls.sh").spawn()?;
        sleep(std::time::Duration::from_secs(1));
        // start the server
        let goat = std::process::Command::new("cargo")
            .args([
                "run",
                "--",
                "--config",
                "./examples/test_config/goatns-test.json",
            ])
            .spawn();
        let mut res = match goat {
            Ok(child) => child,
            Err(error) => {
                log::trace!("Failed to start: {error:?}");
                return Err(error);
            }
        };

        // Construct a new Resolver pointing at localhost
        let localhost: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        let mut config = ResolverConfig::new();
        config.add_name_server(NameServerConfig::new(
            SocketAddr::new(localhost, 25353),
            Protocol::Udp,
        ));
        let resolver = Resolver::new(config, ResolverOpts::default()).unwrap();

        // On Unix/Posix systems, this will read the /etc/resolv.conf
        // let mut resolver = Resolver::from_system_conf().unwrap();

        // Lookup the IP addresses associated with a name.
        // log::trace!(
        //     "{:?}",
        //     resolver.lookup("hello.goat", trust_dns_resolver::proto::rr::RecordType::A)
        // );
        let response = match resolver.lookup_ip("hello.goat") {
            Ok(value) => value,
            Err(error) => {
                res.kill()?;
                panic!("Error resolving hello.goat A {error:?}");
            }
        };

        // There can be many addresses associated with the name,
        //  this can return IPv4 and/or IPv6 addresses
        let address = response.iter().next().expect("no addresses returned!");
        if address.is_ipv4() {
            assert_eq!(address, IpAddr::V4(Ipv4Addr::new(6, 6, 6, 6)));
        } else {
            assert_eq!(
                address,
                IpAddr::V6(Ipv6Addr::new(
                    0x2606, 0x2800, 0x220, 0x1, 0x248, 0x1893, 0x25c8, 0x1946
                ))
            );
        }

        let response = match resolver.lookup(
            "_mqtt._http.hello.goat",
            trust_dns_resolver::proto::rr::RecordType::Unknown(256),
        ) {
            Ok(value) => value,
            Err(error) => panic!("{error:?}"),
        };
        assert!(!response.records().is_empty());
        eprintln!("URL response: {response:?}");

        // clean up
        info!("Killing goatns");
        res.kill()?;
        Ok(())
    }
}
