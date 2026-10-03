#!/bin/bash

# Osprey VSCode Extension Build Script

echo "🚀 Building Osprey VSCode Extension..."

# Check if we're in the right directory
if [ ! -f "package.json" ]; then
    echo "❌ Error: Run this script from the vscode-extension directory"
    exit 1
fi

# Install dependencies
echo "📦 Installing dependencies..."
npm install

# Compile TypeScript
echo "🔨 Compiling TypeScript..."
npm run compile

# Check if compilation was successful
if [ $? -eq 0 ]; then
    echo "✅ Build successful!"
    echo ""
    echo "📋 Next steps:"
    echo "1. Package extension: npm run package"
    echo "2. Install extension: npm run install-extension"
    echo "3. Or manually install: code --install-extension osprey-language-support-0.2.0.vsix"
else
    echo "❌ Build failed!"
    exit 1
fi 