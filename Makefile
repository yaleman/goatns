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
	@fgrep -h "##" $(MAKEFILE_LIST) | fgrep -v fgrep | sed -e 's/\\$$//' | sed -e 's/##/\n\t/'

container:	## Build the docker image locally
container:
	$(eval GITHUB_SHA:=$(shell  git rev-parse HEAD))
	@$(CONTAINER_TOOL) build $(CONTAINER_TOOL_ARGS) \
	--build-arg GITHUB_SHA="${GITHUB_SHA}" \
	-t $(IMAGE_BASE)/server:$(IMAGE_VERSION) $(CONTAINER_BUILD_ARGS) .

build: ## Build binaries
	cargo build

test:
	cargo test

vendor:
	cargo vendor

prep:
	cargo outdated -R
	cargo audit

.PHONY: codespell
codespell: ## Spellchecking, or shaming. Whatever
codespell:
	codespell -c \
	-L crate,unexpect,Pres,pres,ACI,aci,te,ue \
	--skip='./target,./.git,./static_files,./docs/book/*.js,./docs/*.js,./docs/book/FontAwesome/fonts/fontawesome-webfont.svg'
# ,./pykanidm/.venv,./pykanidm/.mypy_cache,./.mypy_cache' \
# --skip='./docs/*,./.git' \
# --skip='./kanidmd_web_ui/src/external,./kanidmd_web_ui/pkg/external' \
# --skip='./kanidmd/lib/src/constants/system_config.rs,./pykanidm/site,./kanidmd/lib/src/constants/*.json'

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
	find . -type f  -not -path './target/*' -not -path '*/.venv/*' \
		-name \*.md \
		-exec deno fmt --check $(MARKDOWN_FORMAT_ARGS) "{}" +

.PHONY: book/format/fix
book/format/fix: ## Fix docs and the book
	find . -type f  -not -path './target/*' -not -path '*/.venv/*' \
		-name \*.md \
		-exec deno fmt  $(MARKDOWN_FORMAT_ARGS) "{}" +

clean_book:
	rm -rf ./target/docs/*
