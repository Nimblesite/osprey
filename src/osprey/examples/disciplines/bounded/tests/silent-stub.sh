#!/bin/sh
# Test double for a compiler that answers `--signature` with nothing at all:
# a module that exports no `module` block has no export signature, and the
# checker must accept it with the empty signature rather than call it a fault.
exit 0
