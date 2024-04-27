.PHONY: help build test book clean_book docs

IMAGE_BASE ?= goatns
IMAGE_VERSION ?= latest
IMAGE_ARCH ?= "linux/amd64,linux/arm64"
CONTAINER_BUILD_ARGS ?=
CONTAINER_TOOL ?= docker
CONTAINER_TOOL_ARGS ?=
MARKDOWN_FORMAT_ARGS ?= --options-line-width=100

.DEFAULT: help
help:
	@fgrep -h "##" $(MAKEFILE_LIST) | \
		fgrep -v fgrep | \
		sed -e 's/\\$$//' | sed -e 's/##/\n\t/'

.PHONY: container
container:	## Build the docker image locally
container:
	$(eval GITHUB_SHA:=$(shell  git rev-parse HEAD))
	@$(CONTAINER_TOOL) buildx build $(CONTAINER_TOOL_ARGS) \
	--build-arg GITHUB_SHA="${GITHUB_SHA}" \
	-t $(IMAGE_BASE)/server:$(IMAGE_VERSION) $(CONTAINER_BUILD_ARGS) .

.PHONY: run_container
run_container: ## Run the container
run_container:
	@$(CONTAINER_TOOL) run $(CONTAINER_TOOL_ARGS) \
	--rm -it \
	--mount "type=bind,src=${HOME}/.config/goatns.json,target=/goatns.json" \
	$(IMAGE_BASE)/server:$(IMAGE_VERSION)

build: ## Build release binaries
	cargo build --release

test: ## run cargo test
test:
	cargo test

vendor: ## vendor dependencies
vendor:
	cargo vendor

prep: ## run cargo outdated and cargo audit
prep: codespell
	cargo outdated -R
	cargo audit

.PHONY: codespell
codespell: ## Spellchecking, or shaming. Whatever
codespell:
	codespell -c \
	-L crate,unexpect,Pres,pres,ACI,aci,te,ue,mut \
	--skip='./target,./.git,./static_files,./docs/book/*.js,./docs/*.js,./docs/book/FontAwesome/fonts/fontawesome-webfont.svg'

doc: ## Build the rust documentation locally
doc:
	cargo doc --document-private-items --no-deps

book: ## Build the book
book: doc
	mdbook build docs
	mv ./docs/book/ ./target/docs/
	mkdir -p ./target/docs/rustdoc/
	mv ./target/doc/* ./target/docs/rustdoc/

.PHONY: book/format
book/format: ## Format docs and the book
book/format:
	find . -type f  -not -path './target/*' -not -path '*/.venv/*' \
		-name \*.md \
		-exec deno fmt --check $(MARKDOWN_FORMAT_ARGS) "{}" +

.PHONY: book/format/fix
book/format/fix: ## Fix docs and the book
book/format/fix:
	find . -type f  -not -path './target/*' -not -path '*/.venv/*' \
		-name \*.md \
		-exec deno fmt  $(MARKDOWN_FORMAT_ARGS) "{}" +

.PHONY: clean_book
clean_book: ## Remove the docs directory contents
clean_book:
	rm -rf ./target/docs/*

.PHONY: semgrep
semgrep: ## Run semgrep
semgrep:
	./semgrep.sh

.PHONY: coverage
coverage: ## Run all the coverage tests
coverage:
	LLVM_PROFILE_FILE="$(PWD)/target/profile/coverage-%p-%m.profraw" RUSTFLAGS="-C instrument-coverage" cargo test $(TESTS)

	rm -rf ./target/coverage/html
	mkdir -p target/coverage/
	grcov . --binary-path ./target/debug/deps/ \
		-s . \
		-t html \
		--branch \
		--ignore-not-existing \
		--ignore '../*' \
		--ignore "/*" \
		--ignore "target/*" \
		-o target/coverage/html
	echo "Coverage report is in ./target/coverage/html/index.html"
