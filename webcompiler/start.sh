#!/bin/bash

# Navigate to webcompiler directory
cd "$(dirname "$0")"

# Check if Docker is installed
if ! command -v docker &> /dev/null; then
    echo "❌ Docker is required but not installed!"
    exit 1
fi

# Container name for management
CONTAINER_NAME="osprey-web-compiler"

# Stop any existing container
docker stop "$CONTAINER_NAME" 2>/dev/null || true
docker rm "$CONTAINER_NAME" 2>/dev/null || true

# Build the Docker image
docker build -t osprey-web-compiler -f Dockerfile ..

# Check if build was successful
if [ $? -ne 0 ]; then
    echo "❌ Docker build failed!"
    exit 1
fi

# Run the container
docker run -d \
    --name "$CONTAINER_NAME" \
    -p 3001:3001 \
    -e NODE_ENV=production \
    -e PORT=3001 \
    --restart unless-stopped \
    --memory=256m \
    --memory-reservation=256m \
    osprey-web-compiler

# Check if container started successfully
if [ $? -ne 0 ]; then
    echo "❌ Failed to start container!"
    exit 1
fi

echo "✅ Container started: http://localhost:3001/api"

# Follow logs
docker logs -f "$CONTAINER_NAME" 