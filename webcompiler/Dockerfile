# Build stage for the Osprey compiler (Rust: osprey) + the pure-C runtime
FROM rust:1-bookworm AS osprey-builder

# Build dependencies: gcc/make for the C runtime archives, OpenSSL for the HTTP runtime
RUN apt-get update && apt-get upgrade -y \
    && apt-get install -y --no-install-recommends \
    git \
    make \
    gcc \
    build-essential \
    libssl-dev \
    openssl \
    pkg-config \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# Copy the Rust workspace + the C runtime sources (root Makefile builds both)
COPY Cargo.toml Cargo.lock Makefile rustfmt.toml clippy.toml /workspace/
COPY scripts /workspace/scripts
COPY crates /workspace/crates
COPY tree-sitter-osprey /workspace/tree-sitter-osprey
COPY compiler/runtime /workspace/compiler/runtime
# The compiler embeds this shared grammar in generated HTML documentation.
COPY website/src/js/osprey-grammar.mjs /workspace/website/src/js/osprey-grammar.mjs
WORKDIR /workspace

# Build the C runtime archives (compiler/lib/lib*_runtime.a)
RUN make _runtime

# Build the Rust compiler binary (release). The same `osprey` binary serves the
# language server over stdio (`osprey lsp`), so there is no separate LSP build.
RUN cargo build --release -p osprey-cli

# Runtime stage
FROM node:20-bookworm

# Install runtime dependencies including LLVM, compiler toolchain, and utilities.
# LLVM/clang MUST be >= 15: osprey-codegen emits the swappable-allocator
# declaration `@osp_alloc` with the `allockind(...)` / "alloc-family" allocator
# attributes (osprey-codegen builder.rs OSP_ALLOC_DECL, [MEM-BACKENDS]), which
# are an LLVM 15 feature. clang-14 rejects them with "expected top-level entity"
# and fails to compile every program. clang-16 is the newest in Debian bookworm.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
    ca-certificates \
    wget \
    llvm-16 \
    llvm-16-dev \
    clang-16 \
    gcc \
    build-essential \
    libssl-dev \
    libssl3 \
    openssl \
    pkg-config \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/* \
    && update-alternatives --install /usr/bin/clang clang /usr/bin/clang-16 60

# Add LLVM16 tools to PATH so Osprey compiler can find them
ENV PATH="/usr/lib/llvm-16/bin:${PATH}"

# Create non-root user for security
RUN groupadd -g 1001 nodejs && \
    useradd -r -u 1001 -g nodejs osprey

# Create app directory
WORKDIR /app

# Copy the built Osprey compiler (Rust binary, installed under the `osprey`
# name the server spawns) from the build stage
COPY --from=osprey-builder /workspace/target/release/osprey /usr/local/bin/osprey

# Copy the C runtime archives osprey links at --run time (the HTTP archive is a
# superset containing the websocket + system objects; osprey links only these two)
COPY --from=osprey-builder /workspace/compiler/lib/libfiber_runtime.a /usr/local/lib/libfiber_runtime.a
COPY --from=osprey-builder /workspace/compiler/lib/libhttp_runtime.a /usr/local/lib/libhttp_runtime.a

# Copy web compiler source
COPY webcompiler/package*.json ./
RUN npm ci --only=production && npm cache clean --force

COPY webcompiler/src ./src

# Create temp directory for compiler operations
RUN mkdir -p /tmp/osprey-temp && chown osprey:nodejs /tmp/osprey-temp

# Change ownership to non-root user
RUN chown -R osprey:nodejs /app

# Make sure the osprey binary is executable
RUN chmod +x /usr/local/bin/osprey

# Set environment variables
ENV DOCKER_ENV=true
ENV NODE_ENV=production
ENV PORT=3001

# Switch to non-root user
USER osprey

# Expose port
EXPOSE 3001

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=40s --retries=3 \
    CMD wget --no-verbose --tries=1 --spider http://localhost:3001/api || exit 1

# Start the server
CMD ["npm", "run", "server"]
