FROM debian:latest AS builder

ARG GITHUB_SHA="$(git rev-parse HEAD)"

LABEL com.goatns.git-commit="${GITHUB_SHA}"

# fixing the issue with getting OOMKilled in BuildKit
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
RUN mkdir /goatns
COPY . /goatns/

WORKDIR /goatns
# install the dependencies
RUN apt-get update && apt-get install -y \
    protobuf-compiler \
    curl \
    clang \
    git \
    build-essential \
    pkg-config \
    mold
# install rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
RUN mv /root/.cargo/bin/* /usr/local/bin/
# do the build bits
ENV CC="/usr/bin/clang"
RUN cargo build --release --bin goatns
RUN chmod +x /goatns/target/release/goatns

FROM gcr.io/distroless/cc-debian12 AS goatns
# # ======================
# https://github.com/GoogleContainerTools/distroless/blob/main/examples/rust/Dockerfile
COPY --from=builder /goatns/target/release/goatns /
COPY --from=builder /goatns/static_files /static_files

# DNS ports
# EXPOSE 15353/udp
# EXPOSE 15353/tcp
# default web API port
# EXPOSE 9000/udp

WORKDIR /
USER nonroot
ENTRYPOINT ["./goatns"]
