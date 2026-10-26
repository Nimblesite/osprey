#!/usr/bin/env bash
# Compile a selected runtime unit with the allocator's ownership shim.
compile_runtime_source() {
    local source=$1
    shift
    local shim=()
    case " ${OSPREY_RUNTIME_SHIMS:-} " in
        *" $source "*) shim=(-include compiler/runtime/osp_arc_shim.h) ;;
    esac
    # `${shim[@]+"${shim[@]}"}`: the stock macOS bash 3.2 treats an empty
    # array's expansion as unbound under `set -u`, and reports that fatal
    # error to the caller as exit 0, so make saw a successful build with no
    # archive. scripts/test-runtime-memory.sh pins both shapes under /bin/bash.
    "$@" ${shim[@]+"${shim[@]}"} "compiler/runtime/$source.c" -o "$scratch/$source.o"
}
