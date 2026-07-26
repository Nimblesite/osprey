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

# Require the migrated suite's TAP assertion to pass.
if echo "$PROGRAM_OUTPUT" | grep -q 'ok 1 - regression scenario completes'; then
    echo "✅ Test PASSED: Regression assertion suite passed"
    exit 0
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
