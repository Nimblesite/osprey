# --- vsix sub-steps ---------------------------------------------------------
# Extension artefacts only. This used to run `$(MAKE) clean`, i.e. `cargo
# clean` plus the C runtime archives, which bought a from-scratch release
# build on every single reinstall and no correctness at all: cargo tracks its
# own inputs, and `_runtime` rebuilds through a stamp that fingerprints the
# compiler, the archiver and every runtime source. The artefacts a reinstall
# genuinely must drop are the extension's own — a previous bundle, a previous
# VSIX, and `out/` (which carries tsconfig.tsbuildinfo, so `tsc -b` cannot
# decide it is up to date and emit nothing).
_vsix_clean:
	cd $(EXT_DIR) && $(RM) bin out dist osprey-*.vsix

# The shared profiler UI the extension links from the lspkit submodule, for CI
# steps that install and lint the extension outside make.
_profiler_ui: $(PROFILER_UI_BUILT)

# Stage the freshly-built Rust binary AND the C runtime archives where the
# extension expects its bundled compiler (bin/<os>-<arch>/), so the VSIX runs
# against THIS build. The compiler locates its runtime archives next to its own
# executable (find_runtime_lib in osprey-cli), so every native runtime variant
# must sit beside the bundled `osprey` for `--run --memory=<mode>` to link.
# shipwright.json is gitignored build output (release.yml writes a stamped copy
# per platform); nothing generated it locally, so the extension's activation-
# time version check either shipped a stale manifest or silently skipped. Local
# builds report 0.0.0-dev, exactly what the committed manifest expects, so the
# root manifest is the truthful local copy. [EDITOR-VERSIONING]
# The editor ships a compiled Osprey package client; installations never compile
# package-manager application sources on the user's machine. [PACKAGE-API]
REGISTRY_CLIENT_SOURCES := $(shell find packages/manager -type f \( -name '*.osp' -o -name 'osprey.toml' \))
target/osprey-pkg: $(BIN) $(REGISTRY_CLIENT_SOURCES)
	$(BIN) build packages/manager/cli -o $@

PACKAGE_WORKSPACE_SOURCES := $(shell find packages/manager/editor packages/native/src packages/vscode/src -type f \( -name '*.osp' -o -name '*.ospml' -o -name 'osprey.toml' \))
target/package-workspace.wasm: $(BIN) $(PACKAGE_WORKSPACE_SOURCES) | _runtime_wasm
	$(BIN) build packages/manager/editor --target=wasm32 -o $@

_vsix_bundle: target/osprey-pkg target/package-workspace.wasm
	@set -e; DEST="$(EXT_DIR)/bin/$(EXT_PLATFORM)"; $(MKDIR) "$$DEST"; \
	cp $(BIN) "$$DEST/osprey"; \
	cp target/osprey-pkg "$$DEST/osprey-pkg"; \
	cp target/package-workspace.wasm "$$DEST/package-workspace.wasm"; \
	cp $(RTB)/libfiber_runtime.a $(RTB)/libhttp_runtime.a \
	   $(RTB)/libfiber_runtime_gc.a $(RTB)/libhttp_runtime_gc.a \
	   $(RTB)/libfiber_runtime_arc.a $(RTB)/libhttp_runtime_arc.a "$$DEST/"; \
	cp shipwright.json $(EXT_DIR)/shipwright.json; \
	echo "  bundled $(BIN) + all native runtime archives -> $$DEST/"

# Refuse missing or stale build artifacts before running the full editor suite.
# Test jobs have no WASI toolchain and must not compile a second package client.
.PHONY: _vsix_verify
_vsix_verify:
	@test -n "$(filter $(RTB)/%,$(NATIVE_RUNTIME_ARCHIVES))"
	@set -e; for INPUT in "$(BIN):osprey" "target/osprey-pkg:osprey-pkg" "target/package-workspace.wasm:package-workspace.wasm" \
	    $(foreach ARCHIVE,$(filter $(RTB)/%,$(NATIVE_RUNTIME_ARCHIVES)),"$(ARCHIVE):$(notdir $(ARCHIVE))"); do \
	  cmp -s "$${INPUT%:*}" "$(EXT_DIR)/bin/$(EXT_PLATFORM)/$${INPUT#*:}" || \
	    { echo "FAIL: missing or stale bundled $${INPUT#*:}; run make _vsix_bundle"; exit 1; }; \
	done
	cd $(EXT_DIR) && node scripts/verify-package-client.mjs

# Package, then prove the VSIX carries the bundled compiler: `vsce` ships
# whatever `.vscodeignore` leaves behind and reports success either way, so a
# packaging regression is invisible until the installed extension quietly falls
# back to whatever `osprey` is on PATH. Same assertion as the `vsix` job in
# .github/workflows/release.yml.
_vsix_package: $(EXT_NODE_DEPS) $(PROFILER_UI_BUILT)
	cd $(EXT_DIR) && npm run package
	@set -e; VSIX=$$(ls -t $(EXT_DIR)/osprey-*.vsix 2>/dev/null | head -1); \
	if [ -z "$$VSIX" ]; then echo "FAIL: npm run package produced no osprey-*.vsix"; exit 1; fi; \
	unzip -l "$$VSIX" | grep -qF "extension/bin/$(EXT_PLATFORM)/osprey" || \
	  { echo "FAIL: $$VSIX carries no extension/bin/$(EXT_PLATFORM)/osprey"; exit 1; }; \
	unzip -l "$$VSIX" | grep -qF "extension/bin/$(EXT_PLATFORM)/osprey-pkg" || \
	  { echo "FAIL: $$VSIX carries no bundled package client"; exit 1; }; \
	unzip -l "$$VSIX" | grep -qF "extension/bin/$(EXT_PLATFORM)/package-workspace.wasm" || \
	  { echo "FAIL: $$VSIX carries no package workspace WASM"; exit 1; }; \
	unzip -l "$$VSIX" | grep -qF "extension/out/client/src/extension.js" || \
	  { echo "FAIL: $$VSIX carries no compiled extension (out/client/src/extension.js)"; exit 1; }

# Install the newest Osprey VSIX into the DEFAULT profile only. `--install-
# extension <file> --force` upgrades that one extension id in place — no
# separate uninstall needed, so the live VSCode reconciles its extension host
# once. It installs exactly that VSIX (the Osprey extension) and no other, and
# does NOT enumerate VSCode profiles, so it can never touch any other extension.
#
# Then verify. `code` prints "was successfully installed" and exits 0 in cases
# where the extension directory does not end up holding this build, and the
# local version string never moves off 0.0.0-dev, so VSCode never offers the
# reload prompt it shows for a real version bump — leaving a running window on
# the previous extension host with nothing on screen saying so. The comparison
# below turns that silent staleness into a hard failure, and the notice below
# that says the one thing the running editor still needs from a human.
_vsix_install:
	@command -v code >/dev/null 2>&1 || \
	  { echo "FAIL: no 'code' CLI on PATH — run VSCode's \"Shell Command: Install 'code' command in PATH\""; exit 1; }
	@set -e; \
	VSIX=$$(ls -t $(EXT_DIR)/osprey-*.vsix 2>/dev/null | head -1); \
	if [ -z "$$VSIX" ]; then echo "FAIL: no osprey-*.vsix in $(EXT_DIR)/"; exit 1; fi; \
	echo "  vsix: $$VSIX"; \
	code --install-extension "$$VSIX" --force; \
	VERSION=$$(node -p "require('$(CURDIR)/$(EXT_DIR)/package.json').version"); \
	INSTALLED="$${VSCODE_EXTENSIONS:-$$HOME/.vscode/extensions}/$(EXT_ID)-$$VERSION"; \
	if [ ! -d "$$INSTALLED" ]; then \
	  echo "FAIL: code reported success but $$INSTALLED does not exist"; exit 1; fi; \
	cmp -s "$(BIN)" "$$INSTALLED/bin/$(EXT_PLATFORM)/osprey" || \
	  { echo "FAIL: $$INSTALLED/bin/$(EXT_PLATFORM)/osprey is not this build ($(BIN))"; exit 1; }; \
	cmp -s "$(EXT_DIR)/out/client/src/extension.js" "$$INSTALLED/out/client/src/extension.js" || \
	  { echo "FAIL: $$INSTALLED/out/client/src/extension.js is not this build"; exit 1; }; \
	echo "  installed $(EXT_ID) $$VERSION -> $$INSTALLED"; \
	echo "  verified: bundled compiler and extension.js are byte-identical to this build"; \
	if pgrep -f "Visual Studio Code.app" >/dev/null 2>&1; then \
	  echo ""; \
	  echo "  VSCode is running. A window opened before this install keeps its old"; \
	  echo "  extension host until reloaded (0.0.0-dev never changes, so no prompt):"; \
	  echo "      Cmd+Shift+P -> Developer: Reload Window"; \
	fi
