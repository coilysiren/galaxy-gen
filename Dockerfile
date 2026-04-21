# syntax=docker/dockerfile:1.7
# -----------------------------------------------------------------------------
# Stage 1: build Rust -> WASM -> JS static bundle
# -----------------------------------------------------------------------------
FROM rust:1.90-bookworm AS builder

# Node (for webpack), binaryen (wasm-opt), curl (for wasm-pack installer)
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      curl ca-certificates binaryen \
 && curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
 && apt-get install -y --no-install-recommends nodejs \
 && rm -rf /var/lib/apt/lists/*

RUN curl -sSf https://rustwasm.github.io/wasm-pack/installer/init.sh | sh

WORKDIR /app

# Cache rust deps: copy manifests and build a shim first.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src/rust && \
    echo "fn main() {}" > src/rust/lib.rs && \
    cargo fetch

# Now the real sources.
COPY src ./src
RUN wasm-pack build --release --out-dir pkg

# Node dependencies + build
COPY package.json package-lock.json ./
COPY webpack.config.js postcss.config.js tsconfig.json ./
RUN npm ci
RUN npm install ./pkg --no-save
RUN npm run build

# -----------------------------------------------------------------------------
# Stage 2: Caddy static server
# -----------------------------------------------------------------------------
FROM caddy:2-alpine AS runtime

COPY --from=builder /app/dist /usr/share/caddy
COPY deploy/Caddyfile /etc/caddy/Caddyfile

ENV PORT=8080
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
  CMD wget -q --spider http://localhost:8080/ || exit 1

CMD ["caddy", "run", "--config", "/etc/caddy/Caddyfile", "--adapter", "caddyfile"]
