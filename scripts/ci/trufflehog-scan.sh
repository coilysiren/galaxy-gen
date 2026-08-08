#!/usr/bin/env bash
# Offline filesystem secret scan over the checkout. Takes the workspace path so
# the caller supplies ${{ github.workspace }} rather than this script guessing.
set -euo pipefail

workspace="${1:-${GITHUB_WORKSPACE:-$PWD}}"
exclude_file="$(mktemp)"
trap 'rm -f "$exclude_file"' EXIT

cat > "$exclude_file" <<'EOF'
(^|/)\.git/
(^|/)\.venv/
(^|/)venv/
(^|/)node_modules/
(^|/)__pycache__/
(^|/)\.mypy_cache/
(^|/)\.pytest_cache/
(^|/)\.ruff_cache/
EOF

docker run --rm \
  -v "${workspace}:/pwd:ro" \
  -v "${exclude_file}:/exclude:ro" \
  trufflesecurity/trufflehog:latest \
  filesystem /pwd \
  --no-verification --no-update --fail \
  --exclude-paths=/exclude
