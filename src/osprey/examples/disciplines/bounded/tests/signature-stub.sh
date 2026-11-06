#!/bin/sh
# Test double for `<compiler> <erased> --signature` inside the `bounded` suite.
#
# The checker's only use of the compiler is to turn the erased module into an
# export signature ([TYPES-DISCIPLINE-PROTOCOL]). Pinning that step to a fixed
# answer keeps the suite's protocol responses byte-exact and independent of
# which compiler build runs it; `_test_disciplines` proves the real
# `<compiler> --signature` path end to end through the roundtrip project.
printf 'signature Rate\n    scale : (int, int) -> int\n    relay : (int) -> int\n    literals : () -> int\n    add : (int, int) -> int\n    inc : (int) -> int\n    ok : () -> int\n'
