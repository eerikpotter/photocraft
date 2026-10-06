# PhotoCraft on a local ICP network

The existing Rust browser app is served by a certified-assets canister. The browser executes
the editor and uses the user's CPU/GPU. PocketIC runs the local canister; it does not render
images. This setup adds hosting, not accounts or cloud document storage.

## Prerequisites

- Node.js 22+ and `icp` CLI 1.5+ (`npm install -g @icp-sdk/icp-cli`).
- Rust 1.95.0 with the browser target. The locked egui 0.36.2 dependencies require 1.95,
  despite the workspace's older declared minimum. Install without changing the global default:

  ```sh
  rustup toolchain install 1.95.0 --profile minimal --target wasm32-unknown-unknown
  ```

- Trunk 0.21.14 (matching the upstream release workflow), installed globally or in
  `.tools/bin`. For example: `cargo +1.95.0 install trunk --version 0.21.14 --locked`.
- Python 3 for packaging and the HTTP smoke check.

The build script pins Rust through `RUSTUP_TOOLCHAIN` for that process only. An explicit
`PHOTOCRAFT_WEB_TOOLCHAIN` override can select a compatible toolchain. It also handles
Trunk's boolean parsing of `NO_COLOR=1` without modifying shell configuration.

## Start and deploy

From the repository root:

```sh
icp network start local -d
ICP_CLI_PLUGIN_COMPUTE_LIMIT_SECS=300 icp deploy frontend -e local
python3 packaging/icp/verify.py
```

`icp.yaml` uses the pinned `@dfinity/static-site@v0.4.0` recipe. Its build step runs Trunk's
optimized, locked browser build, then adds HTTP configuration and attribution notices.
The output in `dist/web` is uploaded to the canister. Browser Wasm is an asset, distinct
from the executable file-serving canister Wasm.

The sync plugin's default 60-second compute budget was too short for Brotli compression of
the roughly 23 MiB application Wasm on the test machine. The command grants 300 seconds
to the local deployment plugin; it does not change canister execution limits. If a sync
times out after installation, retry without rebuilding:

```sh
ICP_CLI_PLUGIN_COMPUTE_LIMIT_SECS=300 icp sync frontend -e local
```

The managed local network uses PocketIC. Port `0` lets the operating system select an
available gateway port, avoiding other projects. Read the current URL from deployment
output or `icp network status local --json`; do not assume port 8000. Use the printed
`frontend.local.localhost` URL or the canister-ID `.localhost` URL.

The smoke check compares the served HTML, generated JavaScript and complete Wasm bytes to
the local build, requires certified response headers, checks Wasm MIME/compression and
cache/CSP headers, and verifies that missing assets return 404. The gateway verifies
certification; byte comparisons alone are not an independent cryptographic audit.

In the browser, check New, brush/undo, local file opening, Save/Export and `?webgl` fallback.
The initial download can be substantial. Editing should not send image data to ICP.

Stop the network when finished:

```sh
icp network stop local
```

Build caches, downloaded local tools and managed network state are ignored. Keep future
mainnet mappings under `.icp/data/` in version control. No mainnet deployment is needed
for this proof of concept.

## HTTP configuration

`packaging/icp/prepare.py` reuses the web package's MIME/cache settings and adds a CSP
with hashes for Trunk's generated inline bootstrap script. Wasm compilation is explicitly
allowed; general JavaScript eval and inline scripts are not. Inline styles remain allowed
because the current HTML/browser UI uses them. Compression and certified chunk delivery
are provided by the static-site canister. No SPA catch-all is needed for this single-page
editor, which has no path-based routing.

The current browser build is single-threaded for CPU algorithms and needs no shared-memory
COOP/COEP headers. Future worker pools require their own browser build and compatibility
testing, especially alongside popup authentication.

## Follow-up work

1. Add OPFS/IndexedDB recovery. Browser Services currently leaves autosave/recover unset;
   `gpu_canvas::now_ms()` returns zero on Wasm and also needs a browser timer implementation.
2. Profile large documents and move long CPU tasks off the UI thread.
3. Add optional Internet Identity and a separate Rust backend for permissions, metadata
   and chunked `.pcraft` checkpoints. Cloud saves need an asynchronous completion flow;
   don't mark a document saved merely because an upload was queued.

The local setup validates hosting/runtime compatibility, not mainnet latency, throughput,
economics, replication or professional Photoshop feature parity.

## Verification on 2026-10-06

At upstream commit `a0d49de0478fbaa91733517297432e7bbe5b479f`, using Rust 1.95.0,
Trunk 0.21.14, ICP CLI 1.5.0 and the managed PocketIC runtime:

- Optimized browser build and upload of 13 assets passed. The browser Wasm is
  24,590,347 bytes (23.45 MiB); upstream's older 18.8 MiB measurement is not this build.
- `packaging/icp/verify.py` passed byte equality, certificate/header, gzip, caching/CSP
  and missing-asset checks through the local gateway.
- Browser initialization selected `BrowserWebGpu`; `?webgl` selected `Gl`. Both rendered
  the editor successfully.
- A synthetic 320 × 240 PPM opened locally. Painting and undo were verified visually.
- Quick Export as PNG reported success in the app, but the in-app browser automation
  did not capture a download event. The resulting downloaded file was not independently
  verified. PSD round trips and large-document responsiveness were not tested here.
- `cargo +1.95.0 xtask layers` passed: 27 crates, no violations. Packaging syntax and
  whitespace checks passed. No Rust application code was changed.

References: [ICP CLI skill](https://skills.internetcomputer.org/.well-known/skills/icp-cli/SKILL.md),
[static-site skill](https://skills.internetcomputer.org/.well-known/skills/static-site/SKILL.md),
and the existing [browser build instructions](development.md#web-build).
