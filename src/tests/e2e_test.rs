#[cfg(test)]
mod tests {

    use std::env;
    use std::net::*;
    use trust_dns_resolver::config::*;
    use trust_dns_resolver::AsyncResolver;

    use crate::servers::udp_server;
    use crate::tests::utils::wait_for_server;

    fn in_github_actions() -> bool {
        env::var("GITHUB_ACTIONS").is_ok()
    }

    #[tokio::test]
    async fn test_full_run() -> Result<(), std::io::Error> {
        if in_github_actions() {
            eprintln!("Skipping this test because it won't work in GHA");
            return Ok(());
        }

        let config = crate::config::ConfigFile::try_as_cowcell(Some(
            &"./examples/test_config/goatns-test.json".to_string(),
        ))?;

        println!("Config as loaded");

        println!("{:?}", config.read().await);

        println!("Starting channels");
        let (agent_sender, datastore_tx, datastore_rx) = crate::utils::start_channels();

        println!("Starting UDP server");
        let udpserver = tokio::spawn(udp_server(
            config.read().await,
            datastore_tx.clone(),
            agent_sender.clone(),
        ));

        println!("Starting database connection pool");
        let connpool = crate::db::get_conn(config.read().await).await.unwrap();

        println!("Starting datastore");

        // start all the things!
        let datastore_manager = tokio::spawn(crate::datastore::manager(
            datastore_rx,
            connpool.clone(),
            None,
        ));

        println!("Starting API Server");
        let apiserver =
            crate::web::build(datastore_tx.clone(), config.read().await, connpool.clone()).await;

        println!("Building server struct");
        let _ = crate::servers::Servers::build(agent_sender)
            .with_datastore(datastore_manager)
            .with_udpserver(udpserver)
            .with_apiserver(apiserver);

        let status_url = config.read().await.status_url();
        wait_for_server(status_url).await;

        // Construct a new Resolver pointing at localhost
        let localhost: std::net::IpAddr = "127.0.0.1".parse().unwrap();
        let mut resolver_config = ResolverConfig::new();
        resolver_config.add_name_server(NameServerConfig::new(
            SocketAddr::new(localhost, 15353),
            Protocol::Udp,
        ));
        let resolver = AsyncResolver::tokio(resolver_config, ResolverOpts::default()).unwrap();

        // Lookup the IP addresses associated with a name.

        println!("Querying hello.goat A");
        let response = match resolver.lookup_ip("hello.goat").await {
            Ok(value) => Some(value),
            Err(error) => {
                eprintln!("Error resolving hello.goat A {error:?}");
                None
            }
        };

        println!("Checking for response");
        if response.is_none() {
            return Ok(());
        }

        // There can be many addresses associated with the name,
        //  this can return IPv4 and/or IPv6 addresses
        let address = response
            .unwrap()
            .iter()
            .next()
            .expect("no addresses returned!");
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
        println!("Successfully got hello.goat A: {:?}", address);

        println!("Querying _mqtt._http.hello.goat URI");
        let response = match resolver
            .lookup(
                "_mqtt._http.hello.goat",
                trust_dns_resolver::proto::rr::RecordType::Unknown(
                    crate::enums::RecordType::URI as u16,
                ),
            )
            .await
        {
            Ok(value) => value,
            Err(error) => panic!("{error:?}"),
        };
        assert!(!response.records().is_empty());
        eprintln!("URL response: {response:?}");

        Ok(())
    }
}
