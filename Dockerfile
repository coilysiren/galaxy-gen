# syntax=docker/dockerfile:1.7

# Stage 1: build Rust -> WASM -> JS static bundle
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

# Stage 2: unprivileged nginx serving the built bundle.
FROM nginxinc/nginx-unprivileged:1.27-alpine AS runtime

COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY --from=builder /app/dist /usr/share/nginx/html
