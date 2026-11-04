#!/bin/sh
# Test double for a compiler that refuses the erased module: the checker must
# turn its standard error into one `check` diagnostic, exit 0 and still answer.
echo "fixture: the erased module does not infer" >&2
exit 1
