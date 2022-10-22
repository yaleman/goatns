FROM rust:latest AS builder

# RUN apt-get update
# RUN apt-get install -y dumb-init
# RUN apt-get clean
RUN rustup default nightly
# ENV RUST_LOG=DEBUG
# ENV CARGO_UNSTABLE_SPARSE_REGISTRY=true
RUN mkdir /goatns
COPY src /goatns/src
COPY benches /goatns/benches
COPY Cargo* /goatns/

WORKDIR /goatns
# RUN cargo fetch -Z sparse-registry
RUN cargo build --release --bin goatns -Z sparse-registry
RUN chmod +x /goatns/target/release/goatns


# FROM gcr.io/distroless/cc as goatns
# # # ======================
# # https://github.com/GoogleContainerTools/distroless/blob/main/examples/rust/Dockerfile
# COPY --from=builder /goatns/target/release/goatns /
# ENV GOATNS_LOG_LEVEL=INFO

# EXPOSE 15353/udp
# EXPOSE 15353/tcp
# WORKDIR /
# USER nonroot
# CMD ["./goatns"]
