# syntax=docker/dockerfile:1.7
# -----------------------------------------------------------------------------
# Stage 1: build Rust -> WASM -> JS static bundle
# -----------------------------------------------------------------------------
FROM rust:1.90-bookworm AS builder

# Node (for webpack) + curl (for wasm-pack and binaryen installers).
# Binaryen is pulled from the upstream release tarball below, NOT apt:
# Debian's binaryen produces wasm-opt output that trips
# `WebAssembly.Table.grow(): failed to grow table by 4` in chromium at
# instantiation time, which wedges the whole JS module graph (React never
# mounts). Matches the pin used by .github/workflows/*.yml.
RUN apt-get update \
 && apt-get install -y --no-install-recommends \
      curl ca-certificates \
 && curl -fsSL https://deb.nodesource.com/setup_20.x | bash - \
 && apt-get install -y --no-install-recommends nodejs \
 && rm -rf /var/lib/apt/lists/*

# Pinned upstream binaryen release. Keep the version in sync with
# .github/workflows/action.yml.
RUN VER=version_119 \
 && curl -sSL "https://github.com/WebAssembly/binaryen/releases/download/${VER}/binaryen-${VER}-x86_64-linux.tar.gz" -o /tmp/binaryen.tgz \
 && tar -xzf /tmp/binaryen.tgz -C /usr/local --strip-components=1 \
 && rm /tmp/binaryen.tgz \
 && wasm-opt --version

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

# Sentry DSN is baked into the JS bundle at build time by webpack's
# DefinePlugin (see webpack.config.js). Browser is the runtime, so a
# k8s env var would be too late. Absent ARG = empty DSN = Sentry stays
# disabled, which is the correct local/dev default.
ARG SENTRY_DSN=""
ENV SENTRY_DSN=${SENTRY_DSN}
RUN npm run build

# -----------------------------------------------------------------------------
# Stage 2: unprivileged nginx serving the built bundle.
# -----------------------------------------------------------------------------
# Self-contained serving image on the coilyco-bridge/deploy static-site
# precedent (atlas, factory-game): nginx-unprivileged, uid 101, listens on
# 8080, TLS terminated upstream by traefik + cert-manager. Built at rollout by
# deploy's services/galaxy-gen/scripts/rollout.sh over this repo's git
# context - replaces the busybox data bundle + initContainer + stock caddy
# shape (galaxy-gen#22, retired with the in-repo deploy surface).
FROM nginxinc/nginx-unprivileged:1.27-alpine AS runtime

COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/dist /usr/share/nginx/html
