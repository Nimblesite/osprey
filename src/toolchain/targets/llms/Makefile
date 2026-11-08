# The language models: run them, train them, fetch their weights. A model's app on a platform lives with
# that platform (the Qwen server on Metal: macos, src/toolchain/targets/macos/tests); its tests with no host at all: src/toolchain/targets/tests.
# Recipes run from the repo root through scripts/make/forward.mk; `make help` at the root lists these too.
ROOT ?= ../../../..
ifeq ($(wildcard scripts/make/forward.mk),)
include $(ROOT)/scripts/make/forward.mk
else
override ROOT := .
include Makefile

.PHONY: gpt2 gpt2-train qwen-pull qwen-pydantic

## gpt2: Run GPT-2 (usage: make gpt2 FLAVOR=default|python PROMPT="..." TOKENS=20 BACKEND=gpu|lists)
## BACKEND=gpu runs the model on the GPU surface: every kernel and product on GPU=auto|metal|opencl, or on the CPU with GPU=host; BACKEND=lists runs the same model over host lists.
## The published weights and merges download once into src/osprey/examples/llms/gpt2/weights (ignored by git).
gpt2: build $(GPT2_DIR)/weights/model.safetensors $(GPT2_DIR)/weights/merges.txt
	@case "$(FLAVOR)" in default|python) ;; *) echo "gpt2 FLAVOR must be default or python" >&2; exit 1 ;; esac
	@printf '%s\n%s\n%s\n' "$(PROMPT)" "$(TOKENS)" "$(BACKEND)" | OSPREY_GPU=$(GPU) ./$(BIN) $(GPT2_GENERATOR) --run --memory=arc

## gpt2-train: One training step of GPT-2 on the prompt: loss, gradients, SGD, loss again (usage: make gpt2-train PROMPT="..." BACKEND=gpu|lists)
gpt2-train: build $(GPT2_DIR)/weights/model.safetensors $(GPT2_DIR)/weights/merges.txt
	@test "$(FLAVOR)" = default || { echo "gpt2-train FLAVOR must be default" >&2; exit 1; }
	@printf '%s\n%s\n' "$(PROMPT)" "$(BACKEND)" | OSPREY_GPU=$(GPU) ./$(BIN) $(GPT2_DIR)/train --run --memory=arc

## qwen-pull: Download the Qwen3.8-27B GGUF (QWEN_QUANT=UD-Q4_K_XL, the plan's default; also UD-Q5_K_M, UD-IQ3_XXS) into ~/.cache/osprey-llm/gguf, resuming a partial file and checking its SHA-256 against Hugging Face's LFS metadata before it is named .gguf.
qwen-pull:
	sh $(QWEN_DIR)/pull.sh

## qwen-pydantic: Local-only real Osprey Qwen coding tests: Pydantic AI writes code, Qwen directly judges it and a broken control. No CI registration.
## Requires QWEN_PYTHON with pydantic-ai-slim[openai]==2.46.0 and the current compiler/runtime; reserves the GPU for up to 900 seconds after building.
qwen-pydantic:
	$(MAKE) _qwen-lanes LANES=pydantic
endif
