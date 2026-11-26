PORT ?= 8080

.PHONY: help build dev serve clean

## help: List available Perception website targets.
help:
	@grep -E '^## ' $(MAKEFILE_LIST) | sed 's/^## //'

## build: Install dependencies if missing and build the Perception Eleventy site into _site/.
build:
	@test -d node_modules || npm install
	npm run build

## dev: Build and serve the Perception website with live reload on http://localhost:$(PORT).
dev:
	@test -d node_modules || npm install
	npx eleventy --serve --watch --port=$(PORT)

## serve: Alias for dev.
serve: dev

## clean: Remove generated _site/ output.
clean:
	npm run clean
