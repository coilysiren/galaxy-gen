.DEFAULT_GOAL := help

.PHONY: help install test-rust build-rust build-wasm build-js-prod dev dev-js dev-rust \
        test-e2e test-e2e-ui test build-docker publish deploy \
        .build-docker .publish .deploy .deploy-ssh

# --- Config (sourced from .ward/ward.yaml) ----------------------------------
dns-name    ?= $(shell yq e '.dns-name' .ward/ward.yaml)
email       ?= $(shell yq e '.email' .ward/ward.yaml)
name        ?= $(shell yq e '.name' .ward/ward.yaml)
port        ?= $(shell yq e '.port' .ward/ward.yaml)
name-dashed ?= $(subst /,-,$(name))
git-hash    ?= $(shell git rev-parse HEAD 2>/dev/null || echo dev)
# Fully-qualified ref into the in-cluster registry. Forgejo Actions builds
# this, pushes it over plain http (the runner's DinD carries
# --insecure-registry=192.168.0.194:30500), and kai-server's containerd
# pulls it via its registries.yaml insecure entry. See
# infrastructure#168, #171.
image-url   ?= 192.168.0.194:30500/$(name-dashed):$(git-hash)
# k3s API endpoint for `.deploy`. Default to the LAN IP, which is rock-solid
# from on-LAN hosts and is in the cert SANs. The kubeconfig's `kai-server`
# context resolves to the tailnet IP (MagicDNS), whose route is flaky/slow and
# i/o-times-out mid-apply. Override (`make deploy k8s-api=https://kai-server:6443`)
# when deploying off-LAN where only the tailnet is reachable. See galaxy-gen#25.
k8s-api     ?= https://192.168.0.194:6443

help: ## Show this help
	@perl -nle'print $& if m{^[a-zA-Z_-]+:.*?## .*$$}' $(MAKEFILE_LIST) | sort | awk 'BEGIN {FS = ":.*?## "}; {printf "\033[36m%-25s\033[0m %s\n", $$1, $$2}'

# --- Local dev -------------------------------------------------------------

install: ## Install Rust, WASM, and JS deps (cargo build + wasm-pack + npm install + playwright install).
	cargo build
	cargo install wasm-pack
	wasm-pack build
	npm install
	npx playwright install chromium

test-rust: ## cargo check + cargo test.
	cargo check
	cargo test -- --color always

build-rust: build-wasm ## Build Rust + WASM (debug).
	cargo build

build-wasm: ## Compile Rust to WASM via wasm-pack (pkg/).
	wasm-pack build

build-js-prod: build-wasm ## Production webpack build.
	npx webpack --config webpack.config.js --mode production

dev: ## Run the rust/wasm watcher and webpack-dev-server concurrently with auto-reload.
	@echo "Starting rust watcher + JS dev server (Ctrl-C stops both)"
	@trap 'kill 0' INT TERM EXIT; \
		cargo watch -w src/rust -w Cargo.toml -s "wasm-pack build --dev" & \
		npx webpack serve --open & \
		wait

dev-js: ## Run only the JS dev server with HMR.
	npx webpack serve --open

dev-rust: ## Run only the Rust/WASM watcher.
	cargo watch -w src/rust -w Cargo.toml -s "wasm-pack build --dev"

test-e2e: build-wasm ## Run Playwright end-to-end tests.
	npm install ./pkg --no-save
	npx playwright test

test-e2e-ui: build-wasm ## Run Playwright tests in UI mode.
	npm install ./pkg --no-save
	npx playwright test --ui

test: test-rust test-e2e ## Run all tests (rust + e2e)

# --- Docker / deploy (same shape as eco-spec-tracker) ----------------------

# --platform linux/amd64 is load-bearing: the cluster node (kai-server) is
# amd64 and the build stage hardcodes the x86_64 binaryen tarball, so the
# image is intrinsically amd64. Without this, a build on an arm64 host (Apple
# Silicon + OrbStack) pushes an arm64 manifest the node rejects at pull time
# with "no match for platform in manifest". CI's runner is already amd64, so
# the flag is a no-op there. See galaxy-gen#23.
.build-docker:
	docker build \
		--platform linux/amd64 \
		--progress plain \
		--build-arg BUILDKIT_INLINE_CACHE=1 \
		--build-arg SENTRY_DSN=$(SENTRY_DSN) \
		--cache-from $(name):latest \
		-t $(name):$(git-hash) \
		-t $(name):latest \
		.

build-docker: .build-docker ## Build the docker image locally with BuildKit cache.

# The built image is now a pure data bundle (busybox + /dist + /Caddyfile),
# not a server - so local serving mirrors the k8s split: extract the payload
# from the bundle image, then serve it with stock caddy over a bind mount.
run-docker: ## Serve the built bundle locally via stock caddy (mirrors the k8s initContainer + caddy split).
	docker rm -f $(name-dashed)-bundle 2>/dev/null || true
	docker create --name $(name-dashed)-bundle $(name):latest >/dev/null
	rm -rf /tmp/$(name-dashed)-dist && mkdir -p /tmp/$(name-dashed)-dist
	docker cp $(name-dashed)-bundle:/dist/. /tmp/$(name-dashed)-dist/
	docker cp $(name-dashed)-bundle:/Caddyfile /tmp/$(name-dashed)-Caddyfile
	docker rm -f $(name-dashed)-bundle >/dev/null
	docker run --rm -e PORT=$(port) -p $(port):$(port) \
		-v /tmp/$(name-dashed)-dist:/usr/share/caddy:ro \
		-v /tmp/$(name-dashed)-Caddyfile:/etc/caddy/Caddyfile:ro \
		caddy:2-alpine

.publish:
	docker tag $(name):$(git-hash) $(image-url)
	docker push $(image-url)

publish: build-docker .publish ## Tag and push the docker image to the in-cluster registry.

.deploy:
	env \
		NAME=$(name-dashed) \
		DNS_NAME=$(dns-name) \
		IMAGE=$(image-url) \
		envsubst < deploy/main.yml | kubectl --server=$(k8s-api) apply -f -
	kubectl --server=$(k8s-api) rollout status deployment/$(name-dashed)-app -n $(name-dashed) --timeout=5m

# Stream the rendered manifest over Tailscale SSH and apply on kai-server.
# Fallback path when the tailnet kubectl route is unavailable.
.deploy-ssh:
	env \
		NAME=$(name-dashed) \
		DNS_NAME=$(dns-name) \
		IMAGE=$(image-url) \
		envsubst < deploy/main.yml | \
		ssh -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile=/dev/null kai@kai-server \
			'kubectl --kubeconfig=/home/kai/.kube/config apply -f -'

deploy: publish .deploy ## Build, push, and apply to the cluster
