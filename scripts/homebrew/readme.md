# Osprey Homebrew Package

> **⚠️ Legacy / local-dev only.** Official releases are published automatically by
> the tag-triggered pipeline (`.github/workflows/release.yml`) to
> [`Nimblesite/homebrew-tap`](https://github.com/Nimblesite/homebrew-tap). Install
> released builds with:
>
> ```bash
> brew install nimblesite/tap/osprey
> ```
>
> The `osprey.rb`, `build-release.sh`, and `setup-tap.sh` files here are kept for
> local experimentation and historical reference; they target the old
> `melbournedeveloper` tap and are **not** used by the release process. See
> [docs/RELEASING.md](../../docs/RELEASING.md).

This directory contains the Homebrew formula for installing the Osprey programming language compiler.

## Installation

### Option 1: From Local Formula (Development)

1. Clone this repository
2. Install using the local formula:
```bash
brew install --build-from-source ./scripts/homebrew/osprey.rb
```

### Option 2: From GitHub (Future)

Once the formula is submitted to Homebrew, you'll be able to install with:
```bash
brew install osprey
```

## Dependencies

The formula only requires this runtime dependency:

- **LLVM** - Low-level code generation and optimization

All build-time dependencies (the Rust toolchain, OpenSSL) are compiled into the pre-built binary and runtime libraries.

## What Gets Installed

- `osprey` binary - The main compiler executable
- `libfiber_runtime.a` - Runtime library for fiber-based concurrency
- `libhttp_runtime.a` - Runtime library for HTTP client/server and WebSocket functionality

## Usage

After installation, you can use the Osprey compiler:

```bash
# Show help
osprey --help

# Compile an Osprey file
osprey your_program.osp
```

## Development

### Testing the Formula Locally

Before submitting to Homebrew, test the formula:

```bash
# Audit the formula
brew audit --strict --new --online ./osprey.rb

# Test installation
brew install --build-from-source ./osprey.rb

# Test the installed binary
osprey --help

# Test uninstallation
brew uninstall osprey
```

### Submitting to Homebrew

#### Step 1: Create a GitHub Release

1. **Build the release package** from the repo root:
   ```bash
   make build
   mkdir -p osprey-darwin-amd64
   cp target/release/osprey osprey-darwin-amd64/
   cp src/toolchain/compiler/lib/libfiber_runtime.a osprey-darwin-amd64/
   cp src/toolchain/compiler/lib/libhttp_runtime.a osprey-darwin-amd64/
   tar -czf osprey-darwin-amd64.tar.gz osprey-darwin-amd64/
   ```

2. **Get the SHA256 checksum**:
   ```bash
   shasum -a 256 osprey-darwin-amd64.tar.gz
   ```

3. **Create a GitHub release**:
   - Go to your Osprey repository on GitHub
   - Click "Releases" → "Create a new release"
   - Tag version: `v0.1.0`
   - Upload the `osprey-darwin-amd64.tar.gz` file
   - Publish the release

#### Step 2: Prepare the Formula

1. **Update the formula** with the real release URL and SHA256:
   ```ruby
   url "https://github.com/your-username/osprey/releases/download/v0.1.0/osprey-darwin-amd64.tar.gz"
   sha256 "actual-sha256-checksum-here"
   ```

2. **Test the formula locally**:
   ```bash
   brew audit --strict --new --online ./osprey.rb
   brew install --build-from-source ./osprey.rb
   brew test osprey
   brew uninstall osprey
   ```

#### Step 3: Submit to Homebrew

1. **Fork homebrew-core**:
   ```bash
   # Fork https://github.com/Homebrew/homebrew-core on GitHub
   git clone https://github.com/YOUR-USERNAME/homebrew-core.git
   cd homebrew-core
   ```

2. **Create a branch**:
   ```bash
   git checkout -b osprey
   ```

3. **Add your formula**:
   ```bash
   cp /path/to/your/osprey.rb Formula/o/osprey.rb
   git add Formula/o/osprey.rb
   ```

4. **Commit with proper message**:
   ```bash
   git commit -m "osprey 0.1.0 (new formula)

   Modern functional programming language compiler with LLVM backend"
   ```

5. **Push and create PR**:
   ```bash
   git push origin osprey
   ```
   Then go to GitHub and create a pull request from your fork to `Homebrew/homebrew-core`

#### Step 4: PR Requirements

Your PR description should include:

```markdown
## osprey 0.1.0 (new formula)

Modern functional programming language designed for clarity, safety, and expressiveness.

### Requirements met:
- [x] License is acceptable (MIT)
- [x] Software is stable and actively maintained
- [x] Formula follows Homebrew style guidelines
- [x] Tests pass locally
- [x] No vendored dependencies
- [x] Pre-built binary (no build dependencies)

### Additional info:
- LLVM dependency for code generation
- Includes runtime libraries for fiber concurrency and HTTP/WebSocket
- Security-hardened runtime with OpenSSL statically linked
```

#### Step 5: Respond to Review

Homebrew maintainers will review and may request changes:
- Fix any audit issues: `brew audit --strict --online osprey`
- Update tests if needed
- Make commits for each requested change
- Be responsive to feedback

#### Common Issues to Avoid:

1. **Wrong formula location**: Must be `Formula/o/osprey.rb` (first letter of name)
2. **Bad commit message**: Should be `osprey 0.1.0 (new formula)`
3. **Missing tests**: Must have working `test do` block
4. **Audit failures**: Run `brew audit` before submitting
5. **Wrong license**: Make sure license is acceptable to Homebrew

### Building Release Packages

To create a release package for Homebrew:

1. **Build** (with the Rust toolchain and OpenSSL installed), from the repo root:
```bash
make build  # C runtime archives (security-hardened) + cargo build --release
```

2. **Package the Release**:
```bash
mkdir -p osprey-darwin-amd64
cp target/release/osprey osprey-darwin-amd64/
cp src/toolchain/compiler/lib/libfiber_runtime.a osprey-darwin-amd64/
cp src/toolchain/compiler/lib/libhttp_runtime.a osprey-darwin-amd64/
tar -czf osprey-darwin-amd64.tar.gz osprey-darwin-amd64/
```

3. **Upload to GitHub Releases** and update the formula URL and SHA256

### Formula Structure

The formula:

1. **Dependencies**: Only LLVM (runtime dependency)
2. **Installation**: Simply copies pre-built binaries and libraries
3. **Testing**: Verifies the compiler works and libraries are installed

### Security Features

The runtime libraries are pre-built with security-hardened compilation flags:
- `_FORTIFY_SOURCE=2` - Buffer overflow protection
- `fstack-protector-strong` - Stack smashing protection
- `ftrapv` - Integer overflow detection
- OpenSSL statically linked (no runtime OpenSSL dependency)

## Troubleshooting

### Installation Failures

If installation fails:

1. Check that LLVM is available: `brew list llvm`
2. Verify the release tarball contains the expected files
3. Check Homebrew permissions: `brew doctor`

### Missing Runtime Libraries

If compiled Osprey programs fail to link:

1. Check libraries are installed: `ls $(brew --prefix)/lib/lib*runtime.a`
2. Verify library paths in your Osprey programs point to the Homebrew installation

### Permission Issues

The formula installs to Homebrew's prefix (usually `/opt/homebrew` on Apple Silicon or `/usr/local` on Intel), which requires appropriate permissions managed by Homebrew.

## Related Projects

- [Osprey Compiler](../../src/toolchain/crates/) - Main compiler source code (Rust)
- [VS Code Extension](../../src/toolchain/vscode-extension/) - IDE support
- [Web Compiler](../../src/toolchain/webcompiler/) - Browser-based playground