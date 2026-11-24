#!/bin/sh
# [MEM-OWNERSHIP-STATIC-VERIFICATION] Clang instruments LLVM IR loads and
# stores only in functions carrying sanitize_address. Link ASan on both
# the cached-object and direct-IR paths of the native driver.
instrument=no
for input do
  case "$input" in
    *.ll)
      awk '/^[[:space:]]*(%[^=]+=[[:space:]]*)?((tail|musttail|notail)[[:space:]]+)?(call|invoke|callbr)[[:space:]]/ && /@osp_(retain|release|release_unique)[[:space:]]*\(/ {
        print FILENAME ":" FNR ": ownership IR still counts references: " $0 > "/dev/stderr"
        exit 1
      }' "$input" || exit 1
      instrument=yes ;;
  esac
done
if [ "${instrument:-}" = yes ]; then
  exec clang -fsanitize=address -mllvm -force-attribute=sanitize_address "$@"
fi
exec clang -fsanitize=address "$@"
