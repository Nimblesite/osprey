# The VS Code extension. Each target runs its recipe in the root Makefile, where it is the
# private `_` target of the same name; `make help` at the root lists these too.
# Assignments use `?=`: editor task panels list `NAME :=` lines as targets.
ROOT ?= ..

.PHONY: vsix-e2e vsix-rebuild-reinstall

## vsix-e2e: VSIX host E2E — package API/transport, sidebar Manage Packages,
##   package workspace browser and SDK host, workbench document host. All selected
##   tests pass. Workbench visual acceptance (WORKBENCH_VISUAL=1) waits on the
##   workbench owner's app-execution hold. Compiling and packaging alone prove nothing visual.
vsix-e2e:
	$(MAKE) -C $(ROOT) _vsix-e2e

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
	$(MAKE) -C $(ROOT) _vsix-rebuild-reinstall
