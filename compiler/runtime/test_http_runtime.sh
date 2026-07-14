#!/bin/bash

# Test script for HTTP runtime
echo "🧪 Compiling and testing HTTP runtime..."

# Compile all the runtime modules and tests
echo "📦 Compiling runtime modules..."

# Detect OpenSSL paths cross-platform
OPENSSL_CFLAGS=""
OPENSSL_LDFLAGS="-lssl -lcrypto"

# Try to find OpenSSL include/lib directories
if [ "$(uname)" = "Darwin" ]; then
    # macOS with Homebrew
    for path in "/opt/homebrew/opt/openssl@3" "/usr/local/opt/openssl@3" "/opt/homebrew/opt/openssl" "/usr/local/opt/openssl"; do
        if [ -d "$path" ]; then
            OPENSSL_CFLAGS="-I${path}/include"
            OPENSSL_LDFLAGS="-L${path}/lib -lssl -lcrypto"
            break
        fi
    done
elif [ "$(uname)" = "Linux" ]; then
    # Linux - usually in standard locations
    if pkg-config --exists openssl 2>/dev/null; then
        OPENSSL_CFLAGS="$(pkg-config --cflags openssl)"
        OPENSSL_LDFLAGS="$(pkg-config --libs openssl)"
    fi
fi

gcc -c http_shared.c -o http_shared.o -pthread $OPENSSL_CFLAGS
gcc -c http_client_runtime.c -o http_client_runtime.o -pthread $OPENSSL_CFLAGS
gcc -c http_server_request.c -o http_server_request.o -pthread $OPENSSL_CFLAGS
gcc -c http_server_response.c -o http_server_response.o -pthread $OPENSSL_CFLAGS
gcc -c http_server_runtime.c -o http_server_runtime.o -pthread $OPENSSL_CFLAGS
gcc -c websocket_client_runtime.c -o websocket_client_runtime.o -pthread $OPENSSL_CFLAGS
gcc -c websocket_server_runtime.c -o websocket_server_runtime.o -pthread $OPENSSL_CFLAGS

if [ $? -ne 0 ]; then
    echo "❌ Runtime compilation failed!"
    exit 1
fi

echo "🧪 Compiling test suite..."
gcc -o test_http_runtime http_runtime_tests.c \
    http_shared.o \
    http_client_runtime.o \
    http_server_request.o \
    http_server_response.o \
    http_server_runtime.o \
    websocket_client_runtime.o \
    websocket_server_runtime.o \
    -L/usr/local/lib -lfiber_runtime \
    -pthread $OPENSSL_CFLAGS $OPENSSL_LDFLAGS

if [ $? -ne 0 ]; then
    echo "❌ Test compilation failed!"
    exit 1
fi

echo "✅ Compilation successful!"
echo ""

# Run the tests
echo "🚀 Running HTTP runtime tests..."
echo ""
./test_http_runtime

if [ $? -ne 0 ]; then
    echo "❌ Tests failed!"
    exit 1
fi

echo ""
echo "🎉 All tests passed! HTTP runtime is working correctly."

# Clean up
rm -f *.o test_http_runtime

echo "✅ Test cleanup complete."
