# syntax=docker/dockerfile:1.7
# -----------------------------------------------------------------------------
# Stage 1: build Rust -> WASM -> JS static bundle
#
# The builder is the promoted dev-base image, the same one CI and dispatched
# agents run in, so the bundle that ships is the bundle those environments
# actually tested. It supplies rust, node, wasm-pack, and a pinned binaryen.
# This stage used to install all four itself, and local dev, GitHub PR CI, and
# this Dockerfile each resolved a different wasm-pack and a different wasm-opt
# (galaxy-gen#74). binaryen is the one that mattered: wasm-pack takes wasm-opt
# from PATH when it finds one and otherwise downloads its own floating latest,
# so the local build optimized every bundle by date rather than by source.
# agentic-os#986 pinned both into the image's Rust payload.
#
# rust-toolchain.toml still pins 1.90.0. rustup honours it on the first cargo
# call, so the gate keeps judging the compiler that builds the artifact.
#
# The moving :release alias is deliberate, matching the aos CI-in-dev-base
# convention (agentic-os#328).
# Pulling it needs a forgejo.coilysiren.me login, which scripts/publish-image.sh
# already performs before `docker build`. A local `ward exec build-docker` needs
# that login too, where the old public base needed none.
# -----------------------------------------------------------------------------
FROM forgejo.coilysiren.me/coilyco-flight-deck/agentic-os:release AS builder

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
# Stage 2: unprivileged nginx serving the built bundle.
# -----------------------------------------------------------------------------
# Self-contained serving image on the shared static-site precedent:
# nginx-unprivileged, uid 101, listens on 8080, TLS terminated upstream by
# traefik + cert-manager. Source Forgejo CI builds and publishes this image.
# It replaces the busybox data bundle + initContainer + stock caddy shape
# (galaxy-gen#22, retired with the in-repo deploy surface).
FROM nginxinc/nginx-unprivileged:1.27-alpine AS runtime

COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/dist /usr/share/nginx/html
