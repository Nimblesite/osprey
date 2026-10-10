#!/bin/bash
set -e

echo "🔧 Running post-creation setup for Osprey dev environment..."

# Install Claude Code and MCP servers
echo "📦 Installing Claude Code and MCP servers..."
sudo npm install -g \
      @modelcontextprotocol/server-filesystem \
      @modelcontextprotocol/server-memory \
      @modelcontextprotocol/server-everything \
      @anthropic-ai/claude-code \
      mcp-smart-crawler \
      @playwright/mcp

echo "📦 Installing GitHub CLI..."
curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | sudo dd of=/usr/share/keyrings/githubcli-archive-keyring.gpg
sudo chmod go+r /usr/share/keyrings/githubcli-archive-keyring.gpg
echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | sudo tee /etc/apt/sources.list.d/github-cli.list > /dev/null
sudo apt-get update
sudo apt-get install gh -y

cd /workspace

echo "🦀 Setting up Rust for vscode user..."
# Ensure Rust is properly set up for vscode user
if [ ! -f ~/.cargo/env ]; then
  echo "🔧 Rust not found for vscode user, installing..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
fi

# Source Rust environment and set default toolchain
source ~/.cargo/env
rustup default stable

# Toolchain components + npm deps for all sub-projects (vscode-extension,
# webcompiler, website) — see the root Makefile `setup` target.
echo "📦 Running make setup..."
make setup

# Build the Osprey compiler (C runtime archives + Rust workspace)
echo "🔨 Building Osprey compiler..."
make build || echo "⚠️ Initial build failed - this is expected on first setup"

echo "🎯 Verifying installation..."
node --version
npm --version
rustc --version || echo "⚠️ Rust not properly installed"
cargo --version || echo "⚠️ Cargo not properly installed"
claude --version || echo "⚠️ Claude Code not installed"

echo "🎉 Post-creation setup complete!"
echo ""
echo "📝 Available commands (run from the repo root):"
echo "  make build      - Build C runtime + Rust compiler + VSCode extension"
echo "  make test       - Run all tests with coverage thresholds"
echo "  make lint       - Run all linters (clippy + extension lint)"
echo "  make install    - Install compiler + runtime archives globally"
echo "  claude          - Run Claude Code"
echo ""
echo "🚀 Ready to develop Osprey!"
