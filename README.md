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
    - [x] add zoneid to FileZoneRecord
    - [x] add recordid (id) to FileZoneRecord
    - [ ] zone
      - [x] create
      - [x] retrieve
      - [x] update
      - [ ] delete 
        - [ ] need to delete all the user ownership
        - [ ] delete all associated records
    - [ ] record
      - [x] create
      - [x] retrieve
      - [ ] update
      - [ ] delete
    - [x] import from json
    - [x] export to json (file-per-zone)
  - [ ] API things
    - [x] move to another web framework (tide-rs)
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