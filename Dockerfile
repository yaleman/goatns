FROM rust:latest AS builder

ARG GITHUB_SHA="${GITHUB_SHA}"

LABEL com.goatns.git-commit="${GITHUB_SHA}"

# fixing the issue with getting OOMKilled in BuildKit
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
RUN mkdir /goatns
COPY . /goatns/

WORKDIR /goatns
RUN cargo build --release --bin goatns
RUN chmod +x /goatns/target/release/goatns

FROM gcr.io/distroless/cc as goatns
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
