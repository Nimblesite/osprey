PANDOC ?= pandoc
EPUBCHECK ?= epubcheck
RSVG_CONVERT ?= rsvg-convert
MAGICK ?= magick
JQ ?= jq
MMDC ?= mmdc
MERMAID_FLAGS := --quiet --configFile styles/mermaid.json --backgroundColor 'rgb(7,13,31)'
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
	assets/diagrams/02-expression-flow.png \
	assets/diagrams/03-inference-trail.png \
	assets/diagrams/03-annotation-choice.png \
	assets/diagrams/03-mismatch-locator.png

.PHONY: check check-examples render-assets epub html release clean

check:
	@$(JQ) -e '(.schemaVersion == 1) and ((.sections | length) == 17)' $(BOOK_JSON) >/dev/null
	@$(JQ) -e '(.schemaVersion == 1) and ((.sources | length) >= 15)' sources.json >/dev/null
	@$(JQ) -e '(.schemaVersion == 1) and ((.figures | length) >= 5)' figures.json >/dev/null
	@for file in $(MANUSCRIPT); do test -f "$$file" || { echo "Missing manuscript file: $$file"; exit 1; }; done
	@$(PANDOC) $(MANUSCRIPT) --from=gfm --to=native >/dev/null

check-examples:
	@test -x "$(OSPREY)" || { echo "Missing compiler: $(OSPREY). Run make build at the repository root."; exit 1; }
	@set -eu; book_tmp=$$(mktemp -d); trap 'rm -r "$$book_tmp"' EXIT; \
		for source in examples/chapter-*/*.osp examples/chapter-*/*.ospml; do \
			test -f "$$source" || continue; \
			$(OSPREY) "$$source" --check >/dev/null; \
			$(OSPREY) "$$source" --run > "$$book_tmp/output"; \
			diff -u "$${source%.*}.expectedoutput" "$$book_tmp/output"; \
		done; \
		for source in examples/chapter-*/failscompilation/*.osp; do \
			test -f "$$source" || continue; \
			if $(OSPREY) "$$source" --check > "$$book_tmp/error" 2>&1; then \
				echo "Expected compiler rejection: $$source"; exit 1; \
			fi; \
			diff -u "$${source%.*}.expectederror" "$$book_tmp/error"; \
		done

render-assets: assets/cover/cover.png $(DIAGRAM_PNGS)

assets/cover/cover.png: assets/cover/cover.svg
	@$(RSVG_CONVERT) --width 1600 --height 2560 --output $@.base.png $<
	@$(MAGICK) $@.base.png \( ../website/src/assets/images/logo.png -resize 184x184 \) -geometry +112+104 -composite $@
	@rm -f $@.base.png

assets/diagrams/%.png: assets/diagrams/%.svg
	@$(RSVG_CONVERT) --width 1600 --height 1000 --output $@ $<

$(filter assets/diagrams/03-%,$(DIAGRAM_PNGS)): assets/diagrams/03-%.png: assets/diagrams/03-%.svg Makefile
	@$(MMDC) $(MERMAID_FLAGS) --input $(@:.png=.mmd) --output $@ --scale 2

.PRECIOUS: assets/diagrams/%.svg
assets/diagrams/%.svg: assets/diagrams/%.mmd styles/mermaid.json
	@$(MMDC) $(MERMAID_FLAGS) --input $< --output $@

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
