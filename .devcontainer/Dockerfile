# Simple, minimal Dockerfile for Osprey development (Rust compiler + C runtime)
FROM debian:bookworm

# Install basic system packages.
# Benchmark suite (make -C benchmarks bench): ghc + ocaml are the comparison toolchains;
# `time` is GNU /usr/bin/time for peak-memory (RSS) measurement. hyperfine is
# installed separately below (not packaged in Debian bookworm).
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    wget \
    git \
    build-essential \
    cmake \
    clang-14 \
    llvm-14 \
    llvm-14-dev \
    libssl-dev \
    pkg-config \
    vim \
    nano \
    tree \
    htop \
    procps \
    jq \
    unzip \
    zip \
    ghc \
    ocaml \
    time \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

# hyperfine — statistical CPU-time benchmarking tool used by `make -C benchmarks bench`.
# Not in Debian bookworm apt; pull the release .deb for the build architecture.
ARG HYPERFINE_VERSION=1.19.0
RUN arch="$(dpkg --print-architecture)" \
    && curl -fsSL -o /tmp/hyperfine.deb \
      "https://github.com/sharkdp/hyperfine/releases/download/v${HYPERFINE_VERSION}/hyperfine_${HYPERFINE_VERSION}_${arch}.deb" \
    && dpkg -i /tmp/hyperfine.deb \
    && rm -f /tmp/hyperfine.deb

# Install Node.js
RUN curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
    && apt-get install -y nodejs

# Install Rust (the compiler toolchain for crates/)
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
    && . ~/.cargo/env \
    && rustup component add clippy rustfmt llvm-tools-preview

# Create vscode user
RUN apt-get update && apt-get install -y sudo \
    && groupadd --gid 1000 vscode \
    && useradd --uid 1000 --gid vscode --shell /bin/bash --create-home vscode \
    && mkdir -p /etc/sudoers.d \
    && echo 'vscode ALL=(ALL) NOPASSWD:ALL' >> /etc/sudoers.d/50-vscode \
    && mkdir -p /home/vscode/.cargo /home/vscode/.rustup \
    && cp -r /root/.cargo/* /home/vscode/.cargo/ || true \
    && cp -r /root/.rustup/* /home/vscode/.rustup/ || true \
    && chown -R vscode:vscode /home/vscode \
    && echo 'source ~/.cargo/env' >> /home/vscode/.bashrc

# Set environment variables
ENV PATH="/home/vscode/.cargo/bin:/usr/lib/llvm-14/bin:${PATH}"

# Set working directory
WORKDIR /workspace

# Switch to vscode user
USER vscode

# Expose ports for development
EXPOSE 8080 3001
