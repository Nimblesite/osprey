PANDOC ?= pandoc
EPUBCHECK ?= epubcheck
RSVG_CONVERT ?= rsvg-convert
MAGICK ?= magick
JQ ?= jq
OSPREY ?= ../target/release/osprey

BOOK_JSON := book.json
MANUSCRIPT := $(shell $(JQ) -r '.sections[].file' $(BOOK_JSON))
EPUB := dist/the-osprey-book-outline.epub
HTML := dist/index.html
DIAGRAM_PNGS := \
	assets/diagrams/00-reading-journey.png \
	assets/diagrams/01-program-anatomy.png \
	assets/diagrams/01-source-to-output.png \
	assets/diagrams/02-value-graph.png \
	assets/diagrams/02-expression-flow.png

.PHONY: check check-examples render-assets epub html release clean

check:
	@$(JQ) -e '(.schemaVersion == 1) and ((.sections | length) == 17)' $(BOOK_JSON) >/dev/null
	@$(JQ) -e '(.schemaVersion == 1) and ((.sources | length) >= 15)' sources.json >/dev/null
	@$(JQ) -e '(.schemaVersion == 1) and ((.figures | length) >= 5)' figures.json >/dev/null
	@for file in $(MANUSCRIPT); do test -f "$$file" || { echo "Missing manuscript file: $$file"; exit 1; }; done
	@$(PANDOC) $(MANUSCRIPT) --from=gfm --to=native >/dev/null

check-examples:
	@test -x "$(OSPREY)" || { echo "Missing compiler: $(OSPREY). Run make build at the repository root."; exit 1; }
	@book_tmp=$$(mktemp -d); trap 'rm -r "$$book_tmp"' EXIT; \
		$(OSPREY) examples/chapter-01/hello.osp --check >/dev/null; \
		$(OSPREY) examples/chapter-01/first-flight.osp --check >/dev/null; \
		$(OSPREY) examples/chapter-01/first-flight.ospml --check >/dev/null; \
		$(OSPREY) examples/chapter-02/flight-log.osp --check >/dev/null; \
		$(OSPREY) examples/chapter-01/hello.osp --run > "$$book_tmp/hello.out"; \
		$(OSPREY) examples/chapter-01/first-flight.osp --run > "$$book_tmp/first-flight.out"; \
		$(OSPREY) examples/chapter-01/first-flight.ospml --run > "$$book_tmp/first-flight-ml.out"; \
		$(OSPREY) examples/chapter-02/flight-log.osp --run > "$$book_tmp/flight-log.out"; \
		diff -u examples/chapter-01/hello.expectedoutput "$$book_tmp/hello.out"; \
		diff -u examples/chapter-01/first-flight.expectedoutput "$$book_tmp/first-flight.out"; \
		diff -u examples/chapter-01/first-flight.expectedoutput "$$book_tmp/first-flight-ml.out"; \
		diff -u examples/chapter-02/flight-log.expectedoutput "$$book_tmp/flight-log.out"

render-assets: assets/cover/cover.png $(DIAGRAM_PNGS)

assets/cover/cover.png: assets/cover/cover.svg
	@$(RSVG_CONVERT) --width 1600 --height 2560 --output $@.base.png $<
	@$(MAGICK) $@.base.png \( ../website/src/assets/images/logo.png -resize 184x184 \) -geometry +112+104 -composite $@
	@rm -f $@.base.png

assets/diagrams/%.png: assets/diagrams/%.svg
	@$(RSVG_CONVERT) --width 1600 --height 1000 --output $@ $<

epub: check check-examples render-assets
	@mkdir -p dist
	@$(PANDOC) $(MANUSCRIPT) \
		--from=gfm \
		--to=epub3 \
		--toc \
		--metadata-file=metadata.yaml \
		--css=styles/epub.css \
		--epub-cover-image=assets/cover/cover.png \
		--output=$(EPUB)
	@$(EPUBCHECK) $(EPUB)

html: check render-assets
	@mkdir -p dist
	@$(PANDOC) $(MANUSCRIPT) \
		--from=gfm \
		--to=html5 \
		--standalone \
		--embed-resources \
		--resource-path=. \
		--toc \
		--metadata-file=metadata.yaml \
		--css=styles/epub.css \
		--output=$(HTML)

release: check check-examples render-assets epub html

clean:
	@rm -f $(EPUB) $(HTML) assets/cover/cover.png $(DIAGRAM_PNGS)
