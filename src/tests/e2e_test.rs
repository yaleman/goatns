#[cfg(test)]
mod tests {
    use log::info;
    use std::net::*;
    use trust_dns_resolver::config::*;
    use trust_dns_resolver::Resolver;

    #[test]
    fn test_full_run() -> Result<(), std::io::Error> {
        // TODO: add a test config and zone file here
        let goat = std::process::Command::new("cargo").args(["run"]).spawn();
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
            SocketAddr::new(localhost, 15353),
            Protocol::Udp,
        ));
        let resolver = Resolver::new(config, ResolverOpts::default()).unwrap();

        // On Unix/Posix systems, this will read the /etc/resolv.conf
        // let mut resolver = Resolver::from_system_conf().unwrap();

        // Lookup the IP addresses associated with a name.
        log::trace!(
            "{:?}",
            resolver.lookup("hello.goat", trust_dns_resolver::proto::rr::RecordType::A)
        );
        let response = resolver.lookup_ip("hello.goat").unwrap();

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
        // clean up
        info!("Killing goatns");
        res.kill()?;
        Ok(())
    }
}
