# https://github.com/casey/just

# list things
default: list

# List the options
list:
    just --list

# Build the docker image locally using buildx
docker_buildx:
    docker buildx build \
        --tag ghcr.io/yaleman/goatns:latest \
        --tag ghcr.io/yaleman/goatns:$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "goatns")  | .version') \
        --tag ghcr.io/yaleman/goatns:$(git rev-parse HEAD) \
        --label org.opencontainers.image.source=https://github.com/yaleman/goatns \
        --label org.opencontainers.image.revision=$(git rev-parse HEAD) \
        --label org.opencontainers.image.created=$(date -u +"%Y-%m-%dT%H:%M:%SZ") \
        --load \
        .

# Build the docker image locally
docker_build:
    docker build \
        --tag ghcr.io/yaleman/goatns:latest \
        --tag ghcr.io/yaleman/goatns:$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "goatns")  | .version') \
        --tag ghcr.io/yaleman/goatns:$(git rev-parse HEAD) \
        --label org.opencontainers.image.source=https://github.com/yaleman/goatns \
        --label org.opencontainers.image.revision=$(git rev-parse HEAD) \
        --label org.opencontainers.image.created=$(date -u +"%Y-%m-%dT%H:%M:%SZ") \
        .

# Publish multi-arch docker image to ghcr.io
docker_publish:
    docker buildx build \
        --platform linux/amd64,linux/arm64 \
        --tag ghcr.io/yaleman/goatns:latest \
        --tag ghcr.io/yaleman/goatns:$(cargo metadata --no-deps --format-version 1 | jq -r '.packages[] | select(.name == "goatns")  | .version') \
        --tag ghcr.io/yaleman/goatns:$(git rev-parse HEAD) \
        --label org.opencontainers.image.source=https://github.com/yaleman/goatns \
        --label org.opencontainers.image.revision=$(git rev-parse HEAD) \
        --label org.opencontainers.image.created=$(date -u +"%Y-%m-%dT%H:%M:%SZ") \
        --push \
        .
