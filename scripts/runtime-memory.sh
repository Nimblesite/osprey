#!/usr/bin/env bash
# Compile a selected runtime unit with the allocator's ownership shim.
compile_runtime_source() {
    local source=$1
    shift
    local shim=()
    case " ${OSPREY_RUNTIME_SHIMS:-} " in
        *" $source "*) shim=(-include compiler/runtime/osp_arc_shim.h) ;;
    esac
    "$@" "${shim[@]}" "compiler/runtime/$source.c" -o "$scratch/$source.o"
}
