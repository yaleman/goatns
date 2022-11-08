FROM rust:latest AS builder

# RUN apt-get update
# RUN apt-get install -y dumb-init
# RUN apt-get clean
# ENV RUST_LOG=DEBUG
# RUN rustup default nightly
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
RUN mkdir /goatns
COPY . /goatns/
# COPY templates /goatns/templates
# COPY benches /goatns/benches
# COPY Cargo* /goatns/

WORKDIR /goatns
# RUN cargo fetch -Z sparse-registry
# RUN cargo build --release --bin goatns -Z sparse-registry
RUN cargo build --release --bin goatns
RUN chmod +x /goatns/target/release/goatns


FROM gcr.io/distroless/cc as goatns
# # ======================
# https://github.com/GoogleContainerTools/distroless/blob/main/examples/rust/Dockerfile
COPY --from=builder /goatns/target/release/goatns /
ENV GOATNS_LOG_LEVEL=INFO

EXPOSE 15353/udp
EXPOSE 15353/tcp
WORKDIR /
USER nonroot
CMD ["./goatns"]
