.PHONY: build test deploy

#
# These targets are used for local testing, the actual
# release targets are defined in the Github action files.
#

DEPLOY_NAME ?= rotel-extension-test
GIT_SHORT_SHA := $(shell git rev-parse --short HEAD)

build:
	cargo lambda build --extension --release

test:
	cargo nextest run

deploy: build
	cargo lambda deploy --extension --compatible-runtimes provided.al2023 \
	  --description "Tag=${GIT_SHORT_SHA}" --binary-name rotel-extension ${DEPLOY_NAME}