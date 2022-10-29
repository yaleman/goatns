.PHONY: help build test book clean_book docs

IMAGE_BASE ?= goatns
IMAGE_VERSION ?= latest
# CONTAINER_TOOL_ARGS ?=
IMAGE_ARCH ?= "linux/amd64,linux/arm64"
CONTAINER_BUILD_ARGS ?=
# Example of using redis with sccache
CONTAINER_TOOL ?= docker

BOOK_VERSION ?= master

.DEFAULT: help
help:
	@fgrep -h "##" $(MAKEFILE_LIST) | fgrep -v fgrep | sed -e 's/\\$$//' | sed -e 's/##/\n\t/'

build/container:	## Build the kanidmd docker image locally
build/kanidmd:
	@$(CONTAINER_TOOL) build $(CONTAINER_TOOL_ARGS) \
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

doc: ## Build the rust documentation locally
doc:
	cargo doc --document-private-items --no-deps

book: ## Build the Kanidm book
book: doc
	mdbook build docs
	mv ./docs/book/ ./target/docs/
	mkdir -p ./target/docs/rustdoc/
	mv ./target/doc/* ./target/docs/rustdoc/

clean_book:
	rm -rf ./target/docs/*
