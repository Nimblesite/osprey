#!/bin/bash

# Osprey Web Compiler API Test
# Tests the local container running on localhost:3001

echo "🧪 Testing Osprey Web Compiler API..."
echo "===================================="

# Define paths to the test files
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OSP_FILE="$SCRIPT_DIR/../tests/regressions/basics/osprey_mega_showcase.test.osp"

# Check if files exist
if [ ! -f "$OSP_FILE" ]; then
    echo "❌ Error: Osprey file not found at $OSP_FILE"
    exit 1
fi

# Read the assertion-driven Osprey test.
OSP_CODE=$(cat "$OSP_FILE")

echo "📄 Loaded Osprey code from: $OSP_FILE"

# Test the local API
echo "Testing local API at http://localhost:3001/api/run"
RESPONSE=$(curl -s -X POST http://localhost:3001/api/run \
  -H 'Content-Type: application/json' \
  -d "{\"code\":$(echo "$OSP_CODE" | jq -Rs .)}")

echo "Response received from API"

# Extract the program output from the JSON response
PROGRAM_OUTPUT=$(echo "$RESPONSE" | jq -r '.programOutput // empty')

if [ $? -ne 0 ]; then
    echo "❌ Test FAILED: Failed to parse JSON response"
    echo "Response: $RESPONSE"
    exit 1
fi

# Verify the response contains expected structure
if echo "$RESPONSE" | jq -e '.success == true' > /dev/null 2>&1; then
    echo "✅ API returned success: true"
else
    echo "❌ Test FAILED: API did not return success: true"
    echo "Response: $RESPONSE"
    exit 1
fi

# Require the showcase's named assertion, TAP plan, and zero-failure summary.
if echo "$PROGRAM_OUTPUT" | grep -Fq 'ok 1 - mega showcase preserves effect, fiber, and tier state' \
    && echo "$PROGRAM_OUTPUT" | grep -Fq '1..1' \
    && echo "$PROGRAM_OUTPUT" | grep -Fq '# tests=1 passed=1 failed=0 skipped=0'; then
    echo "✅ Test PASSED: Mega showcase assertion suite passed"
else
    echo "❌ Test FAILED: Regression assertion result was not present"
    echo ""
    echo "Actual output:"
    echo "=============="
    echo "$PROGRAM_OUTPUT"
    echo ""
    echo "JSON Response:"
    echo "=============="
    echo "$RESPONSE"
    exit 1
fi

# An unused forbidden capability must still be rejected before execution.
# These functions are never called, so the RED run performs no host operation.
check_capability_rejected() {
    local endpoint="$1" code="$2" capability="$3" flavor="${4:-default}"
    local response status
    response=$(curl -sS -w '\n%{http_code}' -X POST "http://localhost:3001/api/$endpoint" \
      -H 'Content-Type: application/json' \
      -d "$(jq -n --arg code "$code" --arg flavor "$flavor" '{code: $code, flavor: $flavor}')")
    status="${response##*$'\n'}"
    response="${response%$'\n'*}"
    if [ "$status" != 422 ] || ! jq -e --arg capability "$capability" \
      '.success == false and (.error | contains("security:") and contains($capability))' <<< "$response" >/dev/null; then
        echo "FAIL: /api/$endpoint did not reject $capability: $response"
        return 1
    fi
}

for endpoint in compile run; do
    check_capability_rejected "$endpoint" 'fn unused() = readFile("osprey-missing-file")
print("safe")' 'readFile' || exit 1
    check_capability_rejected "$endpoint" 'extern fn abort() -> int
print("safe")' 'extern' || exit 1
    check_capability_rejected "$endpoint" 'fn unused() = httpGet(0, "http://127.0.0.1:1")
print("safe")' 'httpGet' || exit 1
    check_capability_rejected "$endpoint" 'fn unused() = cleanupProcess(0)
print("safe")' 'cleanupProcess' || exit 1
done
echo "PASS: compile and run reject filesystem, FFI, HTTP and process capabilities"

# Extension selection must honor Python even without a source marker.
for endpoint in compile run; do
    check_capability_rejected "$endpoint" 'def unused():
    return readFile("unused")
def main():
    print("safe")' 'readFile' python || exit 1
    response=$(curl -sS -X POST "http://localhost:3001/api/$endpoint" \
        -H 'Content-Type: application/json' \
        -d '{"code":"def main():\n    print(\"Python flavor works\")\n","flavor":"python"}')
    jq -e '.success == true' <<< "$response" >/dev/null || { echo "FAIL: Python $endpoint: $response"; exit 1; }
    if [ "$endpoint" = run ]; then
        jq -e '.programOutput == "Python flavor works\n"' <<< "$response" >/dev/null || exit 1
    fi
done
echo 'PASS: Python requests compile and run through the Python frontend'
