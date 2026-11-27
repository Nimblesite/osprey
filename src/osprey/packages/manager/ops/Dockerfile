FROM docker.io/library/rust:1-trixie@sha256:e84068000361bf3d4a5ba2b7647f62893528b1eb9f847d7a23b493f5ce8ea254 AS build

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates clang gcc git libssl-dev make pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
COPY . .
ENV CARGO_TARGET_DIR=/workspace/target
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/workspace/target \
    make _runtime && cargo build --release --locked --manifest-path src/toolchain/Cargo.toml -p osprey-cli \
    && make -C src/osprey/packages registry-hosted-api \
    && install -D -m 0755 target/osprey-registry-hosted /out/osprey-registry-hosted

FROM docker.io/library/debian:trixie-slim@sha256:8aa5a7bf5a1955bddbe7d74fcdd0b91ab30e74362a8b73af672b0481b35b44a0

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates openssl postgresql-client-17 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=build /out/osprey-registry-hosted /usr/local/bin/osprey-registry-hosted
WORKDIR /tmp
USER 65532:65532
EXPOSE 8080 8081
ENTRYPOINT ["/usr/local/bin/osprey-registry-hosted"]
