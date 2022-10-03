FROM rust:latest AS builder

# RUN apt-get update
# RUN apt-get install -y dumb-init
# RUN apt-get clean

RUN mkdir /goatns
COPY src /goatns/src
COPY Cargo* /goatns/

WORKDIR /goatns
RUN cargo build --release
RUN chmod +x /goatns/target/release/goatns

# # ======================
# https://github.com/GoogleContainerTools/distroless/blob/main/examples/rust/Dockerfile
FROM gcr.io/distroless/cc
COPY --from=builder /goatns/target/release/goatns /
ENV GOATNS_LOG_LEVEL=INFO

EXPOSE 15353/udp
EXPOSE 15353/tcp
WORKDIR /
USER nonroot
CMD ["./goatns"]
