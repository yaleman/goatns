# Goat NS

A rusty DNS name server.

Currently designed to be authoritative.

Though, "designed" is a stretch.

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


## Supported record types

- [x] A
- [x] AAAA
- [x] CNAME
- [x] HINFO
- [x] MX
- [x] NS
- [x] PTR
- [x] SOA
- [x] TXT
- [x] CAA

## TODO 

  - [x] allow records with an `@` value for `name` which are apex records.
    - [ ] maaaaybe support flattening of apex records?
  - [ ] record caching instead of loading everything into memory
  - [ ] message length enforcement and testing ([RFC 1035](https://www.rfc-editor.org/rfc/rfc1035#section-2.3.4) 2.3.4. Size limits)
    - [x] labels          63 octets or less
    - [x] names           255 octets or less
    - [x] TTL             positive values of a signed 32 bit number.
    - [x] UDP messages    512 octets or less ? I think this got extended?
  - [x] partial compression based on things
  - [x] TTL handling from the records
  - [ ] TODO: SLIST? <https://www.rfc-editor.org/rfc/rfc1034> something about state handling.
  - [x] lowercase all question name fields - done in the datastore query
  - [x] lowercase all reply name fields
  - [ ] at some point we should be checking that if the zonerecord has a TTL of None, then it should be pulling from the SOA/zone
  - [ ] cleaner ctrl-c handling or shutdown in general