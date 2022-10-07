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

## Notes

- [dnslib](https://github.com/paulc/dnslib/) has some good example data



## TODO 

- [x] allow records with an `@` value for `name` which are apex records.
  - [ ] maaaaybe support flattening of apex records?
- [ ] record caching instead of loading everything into memory
- [ ] TTL handling from the records