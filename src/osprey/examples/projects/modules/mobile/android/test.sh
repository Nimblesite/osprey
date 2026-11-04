#!/usr/bin/env bash
# Requires the isolated live bank started by scripts/bank-mobile-test.py.
set -euo pipefail
app=$(cd "$(dirname "$0")" && pwd)
OSPREY_ANDROID_SKIP_BUILD=1 bash "$app/run.sh" --test
