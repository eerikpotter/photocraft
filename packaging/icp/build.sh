#!/usr/bin/env bash
# Build the existing browser app and prepare files for certified ICP hosting.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export PATH="$ROOT/.tools/bin:$PATH"
# The locked egui 0.36.2 dependencies require Rust 1.95.
export RUSTUP_TOOLCHAIN="${PHOTOCRAFT_WEB_TOOLCHAIN:-1.95.0}"
command -v trunk >/dev/null || { echo "Install Trunk 0.21.14 (see docs/icp.md)." >&2; exit 1; }
(cd "$ROOT/apps/photocraft-web" && env -u NO_COLOR trunk build --release --locked --color never)
python3 "$ROOT/packaging/icp/prepare.py" "$ROOT/dist/web"
