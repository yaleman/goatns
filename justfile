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


# Serve the book
book:
	cd docs && mdbook serve

# Run a local debug instance
run:
	cargo run -- server


# Run all the checks
check: codespell clippy test doc_check


# Spell check the things
codespell:
	codespell -c \
	--ignore-words .codespell_ignore \
	--skip='./Makefile' \
	--skip='./target' \
	--skip='./Cargo.lock' \
	--skip='./tarpaulin-report.html' \
	--skip='./static_files/*' \
	--skip='./docs/*,./.git' \
	--skip='./plugins/*'

# Ask the clip for the judgement
clippy:
	cargo clippy --all-features --all-targets --quiet

test:
	cargo test --quiet

# Things to do before a release
release_prep: check doc semgrep
	cargo deny check
	cargo build --release --quiet

# Semgrep things
semgrep:
	semgrep ci --config auto $OUTPUT \
	--exclude-rule "yaml.github-actions.security.third-party-action-not-pinned-to-commit-sha.third-party-action-not-pinned-to-commit-sha" \
	--exclude-rule "generic.html-templates.security.var-in-script-tag.var-in-script-tag" \
	--exclude-rule "javascript.express.security.audit.xss.mustache.var-in-href.var-in-href" \
	--exclude-rule "python.django.security.django-no-csrf-token.django-no-csrf-token" \
	--exclude-rule "python.django.security.audit.xss.template-href-var.template-href-var" \
	--exclude-rule "python.django.security.audit.xss.var-in-script-tag.var-in-script-tag" \
	--exclude-rule "python.flask.security.xss.audit.template-href-var.template-href-var" \
	--exclude-rule "python.flask.security.xss.audit.template-href-var.template-href-var"

# Build the rustdocs
doc:
	cargo doc --document-private-items

# Run coverage analysis with tarpaulin (HTML output)
coverage:
	cargo tarpaulin --out Html
	@echo "Coverage file at file://$(PWD)/tarpaulin-report.html"

# Run cargo tarpaulin and upload to coveralls
coveralls:
	cargo tarpaulin --coveralls $COVERALLS_REPO_TOKEN

# Check docs format
doc_check:
	find . -type f  \
		-not -path 'CLAUDE.md' \
		-not -path './target/*' \
		-not -path './docs/*' \
		-not -path '*/.venv/*' -not -path './vendor/*'\
		-not -path '*/.*/*' \
		-name \*.md \
		-exec deno fmt --check --options-line-width=100 "{}" +

# Fix docs formatting
doc_fix:
	find . -type f  -not -path './target/*' -not -path '*/.venv/*' -not -path './vendor/*'\
		-name \*.md \
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

# Build the book and organize docs (alternative to simple book command)
build_book: doc
	mdbook build docs
	mv ./docs/book/ ./target/docs/
	mkdir -p ./target/docs/rustdoc/
	mv ./target/doc/* ./target/docs/rustdoc/

# Format docs and check them
book_format:
	find . -type f \
		-not -path './CLAUDE.md' \
		-not -path './target/*' \
		-not -path '*/.venv/*' \
		-name \*.md \
		-exec deno fmt --check --options-line-width=100 {} +

# Fix docs formatting
book_format_fix:
	find . -type f \
		-not -path './target/*' \
		-not -path '*/.venv/*' \
		-name \*.md \
		-exec deno fmt --options-line-width=100 {} +

# Remove the docs directory contents
clean_book:
	rm -rf ./target/docs/*

