# GoatNS

![GoatNS Logo](./static_files/img/apple-touch-icon.png)

Yet another authoritative DNS name server. But with goat references.

## Features

- DNS over HTTPS (RFC8484) on `/dns-query`
- Web API/UI for management
  - OIDC Authentication to the Web UI
  - Token auth for API endpoints

## Configuration

Look at `zones.json` and `goatns.example.json` for examples.

The configuration file's fields are best found here:
<https://goatns.dotgoat.net/rustdoc/goatns/config/struct.ConfigFile.html>. Note that the
`ip_allow_list` field is a nested map.

## Running in Docker

There's a docker container at `ghcr.io/yaleman/goatns:latest` and a docker-compose.yml file in the
repository if that's your thing.

## Help?

Found a bug, want to change something, the sky is falling?
[Create an issue!](https://github.com/yaleman/goatns/issues/new).

Wondering how something works, need a chat, or are curious there's so many goat references?
[Discussions are great for that](https://github.com/yaleman/goatns/discussions).

## Built in Rust

Thanks to some great packages:

- Networking features use [tokio](https://crates.io/crates/tokio)
- DNS Packets are largely decoded/encoded with
  [packed_struct](https://crates.io/crates/packed_struct)
- HTTP things use:
  - [tide](https://crates.io/crates/tide)
  - [Askama](https://crates.io/crates/askama)
  - [Bootstrap 5](https://getbootstrap.com)
  - [Feather icons](https://feathericons.com)
- Database - [sqlx](https://crates.io/crates/sqlx) for async SQLite goodness.
- Logging - [flexi_logger](https://crates.io/crates/flexi_logger)

## Rust Crate Documentation

Auto-generated and available here:
[https://yaleman.github.io/goatns/rustdoc/goatns](https://yaleman.github.io/goatns/rustdoc/goatns/)

## Testing

Rust tests are run using cargo.

```shell
cargo test
```

A handy load testing tool is [dnsblast](https://github.com/jedisct1/dnsblast). This'll run 50,000
"valid" queries, 1500 packets per second, to port 15353:

```shell
./dnsblast 127.0.0.1 50000 1500 15353
```

Or if you want to fuzz the server and test that it doesn't blow up:

```shell
./dnsblast fuzz 127.0.0.1 50000 1500 15353
```

## Supported request/record types

This list is now [in the book](https://goatns.dotgoat.net/rrtypes.html).

## Additional thanks

- "Home" by Kiki Rizky from [Noun Project](https://thenounproject.com/browse/icons/term/home/)
