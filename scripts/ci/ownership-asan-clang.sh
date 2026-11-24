#!/bin/sh
# [MEM-OWNERSHIP-STATIC-VERIFICATION] Clang instruments LLVM IR loads and
# stores only in functions carrying sanitize_address. Link ASan on both
# the cached-object and direct-IR paths of the native driver.
for input do
  case "$input" in
    *.ll) exec clang -fsanitize=address -mllvm -force-attribute=sanitize_address "$@" ;;
  esac
done
exec clang -fsanitize=address "$@"
