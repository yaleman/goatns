# GoatNS

A rusty DNS name server.

Currently designed to be authoritative.

Though, "designed" is a stretch.

## Crate Documentation

Auto-generated and available here: [https://yaleman.github.io/goatns/rustdoc/goatns](https://yaleman.github.io/goatns/rustdoc/goatns/)

## Configuration

Look at `zones.json` and `goatns.json` for examples.

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

- [x] A
- [x] AAAA
- [ ] AXFR
  - [ ] add an allow-list in the config file (CIDRs)
- [x] CAA
- [x] CNAME
- [x] HINFO
- [X] LOC
- [ ] MAILB
- [ ] MB
- [ ] MD
- [ ] MF
- [ ] MG
- [ ] MINFO
- [ ] MR
- [x] MX
- [ ] NAPTR
- [x] NS
- [x] PTR
- [x] SOA
- [x] TXT
- [x] URI ([RFC 7553](https://www.rfc-editor.org/rfc/rfc7553))
- [ ] WKS

## TODO 

  - [ ] record storage in a DB and caching instead of loading everything into memory
  - [ ] support all sorts of records with classes, because bleh
  - [ ] rewrite ttl handling so you don't *have* to specify it per-record and it uses zone data
  - [ ] response caching to save the lookups and parsing
    - [ ] concread?
  - [ ] good e2e tests for LOC records from zone files
  - [ ] cleaner ctrl-c handling or shutdown in general
    - [ ] thinking I need to set up a broadcast tokio channel which the threads consume and shutdown from 
      - [ ] `datastore` just needs to know to write out anything it's working on at the time, which may need an internal state flag for "are we shutting down" so any new write transactions are rejected
  - [ ] maaaaybe support flattening of apex records?
  - [ ] support VERSION/VERSION.BIND requests
    - [x] allow list config
    - [ ] build the response packets in a nice way that doesn't blow up
  - [ ] API things
    - [ ] try and fix rocket's horrible logging
    - [ ] [Oauth](https://docs.rs/rocket_oauth2/latest/rocket_oauth2/)
