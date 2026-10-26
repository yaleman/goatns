# https://github.com/casey/just

# list things
[private]
default:
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



# Check docs format
doc_check:
	find . -type f  \
		-not -path 'CLAUDE.md' \
		-not -path 'AGENTS.md' \
		-not -path './.*' \
		-not -path './target/*' \
		-not -path './docs/*' \
		-not -path '*/.venv/*' -not -path './vendor/*'\
		-not -path '*/.*/*' \
		-name \*.md \
		-exec deno fmt --check --options-line-width=100 "{}" +

# Fix docs formatting
doc_fix:
	find . -type f  -not -path './target/*' -not -path '*/.venv/*' -not -path './vendor/*' -name '*.md' \
		-exec deno fmt --options-line-width=100 "{}" +

# Run trivy on the image
trivy_image:
	trivy image ghcr.io/yaleman/goatns:latest --scanners misconfig,vuln,secret

# Run trivy on the repo
trivy_repo:
	trivy repo $(pwd) --skip-dirs 'target/**' --skip-files .envrc -d

# Build release binaries
build:
	cargo build --quiet --release

# Vendor dependencies
vendor:
	cargo vendor

# Run cargo outdated and cargo audit
prep: codespell
	cargo outdated -R
	cargo audit


# Run the container with config mount
run_container:
	docker run \
		--rm -it \
		--mount "type=bind,src=${HOME}/.config/goatns.json,target=/goatns.json" \
		goatns/server:latest
