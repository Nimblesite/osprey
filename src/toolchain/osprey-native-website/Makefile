.PHONY: build dev serve test clean

## build: Build the standalone Osprey Native Eleventy website into _site/.
build:
	npm run build

## dev: Run Eleventy dev server with live reload.
dev:
	npm run dev

## serve: Build and serve _site/ locally on port 4173.
serve: build
	npm run serve

## test: Build _site/ and run the Playwright E2E suite.
test: build
	npm test

## clean: Remove generated _site/ and test artifacts.
clean:
	rm -rf _site test-results playwright-report
