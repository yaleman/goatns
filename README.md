# GoatNS

Yet another authoritative DNS name server. But with goat references.

- Built in Rust, thanks to some great packages
  - DNS features use [tokio](https://crates.io/crates/tokio) and [packed_struct](https://crates.io/crates/packed_struct)
  - HTTP things use [tide](https://crates.io/crates/tide) / [Askama](https://crates.io/crates/askama)
  - Database - [sqlx](https://crates.io/crates/sqlx) for async sqlite goodness.
  - Logging - [flexi_logger](https://crates.io/crates/flexi_logger)

## Crate Documentation

Auto-generated and available here: [https://yaleman.github.io/goatns/rustdoc/goatns](https://yaleman.github.io/goatns/rustdoc/goatns/)

## Configuration

Look at `zones.json` and `goatns.json` for examples.

The configuration file's fields are best found here: <https://goatns.dotgoat.net/rustdoc/goatns/config/struct.ConfigFile.html>. Note that the `ip_allow_list` field is a nested map.

## Testing

Rust tests are run using cargo.

```shell
cargo test
```

A handy test tool is [dnsblast](https://github.com/jedisct1/dnsblast). This'll run 50,000 "valid" queries, 1500 packets per second, to port 15353:

```shell
./dnsblast 127.0.0.1 50000 1500 15353
```

Or if you want to fuzz the server and test that it doesn't blow up:

```shell
./dnsblast fuzz 127.0.0.1 50000 1500 15353
```

## Running in Docker

There's a dockerfile at `ghcr.io/yaleman/goatns:latest` and a docker-compose.yml file if that's your thing.

## Supported request/record types

This list is now [in the book](https://goatns.dotgoat.net/rrtypes.html).

## TODO 

  - [ ] set config.hostname as authority on SOA records
  - [ ] test records for every rrtype
  - [ ] API things
    - [ ] Oauth for management/UI things
  - [ ] support all record-classes
  - [ ] rewrite ttl handling so you don't *have* to specify it per-record and it uses zone data
   - [?] SOA minimum overrides RR TTL - RFC1035 3.3.13 - "Whenever a RR is sent in a response to a query, the TTL field is set to the maximum of the TTL field from the RR and the MINIMUM field in the appropriate SOA." - this is done in the database view currently
   - [ ] write tests for this
  - [ ] response caching to save the lookups and parsing
    - [ ] concread?
  - [ ] good e2e tests for LOC records from zone files
    - [ ] a converter from InternalResourceRecord::LOC to FileZoneRecord::LOC
  - [ ] cleaner ctrl-c handling or shutdown in general
    - [ ] thinking I need to set up a broadcast tokio channel which the threads consume and shutdown from 
      - [ ] `datastore` just needs to know to write out anything it's working on at the time, which may need an internal state flag for "are we shutting down" so any new write transactions are rejected
  - [ ] maaaaybe support flattening of apex records?
  - [ ] stats?
  - [ ] support VERSION/VERSION.BIND requests
    - [x] allow list config
    - [ ] build the response packets in a nice way that doesn't blow up