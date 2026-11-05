# One definition per target: a folder Makefile holds its recipes, and this file sends any goal given in
# that folder back to the same Makefile at the repo root, where the recipes' paths and private steps resolve.
FOLDER := $(patsubst $(abspath $(ROOT))/%,%,$(CURDIR))

.PHONY: forward
forward:
	+@$(MAKE) --no-print-directory -C $(ROOT) -f $(FOLDER)/Makefile $(or $(MAKECMDGOALS),help)

Makefile $(ROOT)/scripts/make/forward.mk: ;

%: forward
	@:
