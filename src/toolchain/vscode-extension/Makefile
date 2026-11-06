# The VS Code extension. Recipes run from the repo root through scripts/make/forward.mk, with the root's
# configuration and private steps; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: vsix-e2e vsix-rebuild-reinstall

## vsix-e2e: VSIX host E2E — package API/transport, sidebar Manage Packages,
##   package workspace browser and SDK host, workbench document host. All selected
##   tests pass. Workbench visual acceptance (WORKBENCH_VISUAL=1) waits on the
##   workbench owner's app-execution hold. Compiling and packaging alone prove nothing visual.
vsix-e2e: $(EXT_NODE_DEPS)
	OSPREY_TEST_GREP='package |direct dependency reports|serialized per-project mutation|command registration|sidebar Manage Packages button|package workspace|compiled SDK renderer E2E|workbench document host wiring$(if $(WORKBENCH_VISUAL),|workbench VSIX browser|workbench VSIX visual comparison,)' npm test --prefix $(EXT_DIR)

## vsix-rebuild-reinstall: Rebuild the compiler and reinstall the Osprey VSCode
##      extension in place, bundling the freshly-built Rust binary as `osprey`.
##      Touches ONLY the Osprey extension ($(EXT_ID)) in the DEFAULT profile —
##      never another extension, never another VSCode profile. macOS only.
##      ONE `code` invocation (install --force, no separate uninstall) so the
##      running VSCode reconciles its extension host exactly once, not twice.
##      Every step is verified: the VSIX must contain the bundled compiler and
##      the installed extension must be byte-identical to this build, so
##      "installed" is a fact rather than a claim `code` printed.
vsix-rebuild-reinstall:
	$(MAKE) _vsix_clean
	$(MAKE) build
	$(MAKE) _vsix_bundle
	$(MAKE) _vsix_package
	$(MAKE) _vsix_install
endif
