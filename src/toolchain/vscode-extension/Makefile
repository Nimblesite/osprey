# The VS Code extension. Recipes run from the repo root through scripts/make/forward.mk, with the root's
# configuration and private steps; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: vsix-e2e vsix-rebuild-reinstall-macos vsix-rebuild-reinstall-linux vsix-rebuild-reinstall-windows

## vsix-e2e: VSIX host E2E — package API/transport, sidebar Manage Packages,
##   package workspace browser and SDK host, workbench document host. All selected
##   tests pass. Workbench visual acceptance (WORKBENCH_VISUAL=1) waits on the
##   workbench owner's app-execution hold. Compiling and packaging alone prove nothing visual.
vsix-e2e: $(EXT_NODE_DEPS)
	OSPREY_TEST_GREP='package |direct dependency reports|serialized per-project mutation|command registration|sidebar Manage Packages button|package workspace|compiled SDK renderer E2E|workbench document host wiring$(if $(WORKBENCH_VISUAL),|workbench VSIX browser|workbench VSIX visual comparison,)' npm test --prefix $(EXT_DIR)

## vsix-rebuild-reinstall-macos: Rebuild the compiler and reinstall the Osprey
##      VSCode extension in place on Apple Silicon macOS (darwin-arm64),
##      bundling the freshly-built Rust binary as `osprey`.
##      Every vsix-rebuild-reinstall-* target touches ONLY the Osprey extension
##      ($(EXT_ID)) in the DEFAULT profile — never another extension, never
##      another VSCode profile — and refuses to run on any other host, since it
##      bundles the binaries this machine builds. ONE `code` invocation
##      (install --force, no separate uninstall) so the running VSCode
##      reconciles its extension host exactly once, not twice. Every step is
##      verified: the VSIX must contain the bundled compiler and the installed
##      extension must be byte-identical to this build, so "installed" is a fact
##      rather than a claim `code` printed.
vsix-rebuild-reinstall-macos:
	$(call _vsix_require_host,darwin-arm64,Apple Silicon macOS)
	$(MAKE) _vsix_rebuild_reinstall

## vsix-rebuild-reinstall-linux: The same on Linux (linux-x64 or linux-arm64).
vsix-rebuild-reinstall-linux:
	$(call _vsix_require_host,linux-%,Linux)
	$(MAKE) _vsix_rebuild_reinstall

## vsix-rebuild-reinstall-windows: The same on Windows (win32-x64), built in
##      MSYS2's UCRT64 environment as release.yml builds it. Run it from
##      PowerShell, Git Bash or a UCRT64 shell: outside UCRT64 it re-runs itself
##      there ($(MSYS2_ROOT), Windows PATH inherited for cargo, node and code).
##      Needs `pacman -S make unzip mingw-w64-ucrt-x86_64-{gcc,clang,llvm}`.
##      The C runtime is built with gcc and the bundled package client is
##      linked with the UCRT64 clang (OSPREY_CC overrides it).
vsix-rebuild-reinstall-windows:
	$(call _vsix_require_host,win32-%,Windows)
ifeq ($(MSYSTEM),UCRT64)
	$(MAKE) _vsix_windows_tools
	OSPREY_CC="$${OSPREY_CC:-$$(cygpath -m /ucrt64/bin/clang.exe)}" \
	  $(MAKE) _vsix_rebuild_reinstall CC=gcc AR=ar
else ifeq ($(MSYSTEM),)
	$$env:MSYSTEM = 'UCRT64'; $$env:MSYS2_PATH_TYPE = 'inherit'; $$env:CHERE_INVOKING = '1'; & '$(MSYS2_ROOT)/usr/bin/bash.exe' -lc '/usr/bin/make -C src/toolchain/vscode-extension vsix-rebuild-reinstall-windows'; exit $$LASTEXITCODE
else
	MSYSTEM=UCRT64 MSYS2_PATH_TYPE=inherit CHERE_INVOKING=1 \
	  '$(MSYS2_ROOT)/usr/bin/bash.exe' -lc '/usr/bin/make -C src/toolchain/vscode-extension vsix-rebuild-reinstall-windows'
endif
endif
