# PhotoCraft on ICP

The `codex/shared-account-files` branch builds on the internal `sovereign-cloud` SDK extraction
and adds a shared portal account and file catalog while preserving this standalone deployment workflow. When checked out
under `subnet-cloud/apps/photocraft`, the parent workspace can also serve all four Craft apps
from one frontend. Its `scripts/dev.py` deploys to a separate local network. The SDK is vendored
with a hash manifest; update it through the parent `scripts/sync-sdk.py`, not by hand.
The backend is now a checked snapshot of the parent's `services/sovereign-cloud`, updated
through `scripts/sync-service.py`. Use the parent deployment to update the subnet.ee workspace;
the commands below still deploy this standalone PhotoCraft project independently.

The optional cloud build runs the same Rust editor in the browser and adds Internet Identity,
owner-only cloud files and revision storage. A certified static-site canister serves the
application; a separate Rust canister stores file manifests and chunked `.pcraft` files.
See [ICP Cloud Architecture](ICP_CLOUD_ARCHITECTURE.md) for the thesis, boundaries and roadmap.

## Prerequisites

- Node.js 22+, ICP CLI 1.5+ and `ic-wasm`:

  ```sh
  npm install -g @icp-sdk/icp-cli @icp-sdk/ic-wasm
  ```

- Rust 1.95.0, matching the locked egui requirements:

  ```sh
  rustup toolchain install 1.95.0 --profile minimal --target wasm32-unknown-unknown
  rustup component add --toolchain 1.95.0 rustfmt clippy
  ```

- Trunk 0.21.14, installed globally or under `.tools/bin`:

  ```sh
  cargo +1.95.0 install trunk --version 0.21.14 --locked
  ```

- Python 3 for packaging and the HTTP verification script.

The editor and cloud service have separate Cargo workspaces/lockfiles. `icp/backend/canister.yaml`
uses the official Rust recipe; the root `icp.yaml` composes it with certified static hosting.
The web build enables `icp-cloud` only through `packaging/icp/build.sh`. Normal upstream web
and native builds do not include the cloud adapter.

## Run locally

From the repository root:

```sh
icp network start local -d
RUSTUP_TOOLCHAIN=1.95.0 ICP_CLI_PLUGIN_COMPUTE_LIMIT_SECS=300 icp deploy -e local
python3 packaging/icp/verify.py
```

Deploy both canisters so the frontend receives `PUBLIC_CANISTER_ID:cloud`. Port `0` chooses an
available gateway port. Use the deployment's `frontend.local.localhost` URL, or find it with
`icp network status local --json`; never assume port 8000. Keep the same origin when returning
to your account. The current managed local network accepts mainnet Internet Identity delegations,
so local testing can use `id.ai` without creating a second identity service.

`ICP_CLI_PLUGIN_COMPUTE_LIMIT_SECS=300` increases the local asset sync plugin's compression
budget. It does not change canister execution limits. If installation succeeds but asset
compression times out, retry only the sync:

```sh
ICP_CLI_PLUGIN_COMPUTE_LIMIT_SECS=300 icp sync frontend -e local
```

### Browser demonstration

1. Open/create a document using the normal editor.
2. Choose **File → Sovereign Cloud → Save**. Sign in with Internet Identity if needed.
3. The first save asks for a file name. Select **Save** once; the naming dialog is replaced by
   the file manager and progress. The loader appears before encoding, followed by upload
   progress and **Finishing save…**. Wait for the dated **Saved** activity confirmation.
4. Edit and choose **File → Sovereign Cloud → Save** again: changes upload directly, without another
   confirmation. Repeating a save at the same document revision reports **already saved**
   without uploading. Use
   **File → Sovereign Cloud → Save As…** to create a new file; subsequent cloud saves update that new
   file. The File menu has one **Sovereign Cloud** section below the local save block, with
   one ICP mark in its heading and **Save**, **Save As…**, **Open…** below. File management
   opens through the section’s Open action; there is no separate toolbar launcher.
5. Reload, sign in if necessary, and choose **File → Sovereign Cloud → Open…**. **My files** lists
   completed saves in compact rows with a name, latest save date/time and **Open file**.
   Click the name/date area to expand version history; **Open file** opens the latest version
   directly. The resizable cloud window scrolls when needed, without a second nested file list.
   File and version timestamps use your device's local timezone. Activity messages are
   dismissible and expire after eight seconds; errors stay visible until dismissed.
6. Use a second browser/device at the same application origin and sign in to the same II account.
7. Test interrupted saves: the previous version remains available; **Retry save** below the file list
   resumes the captured in-memory snapshot. **Dismiss attempt** lets you save current
   edits instead. In **My files → Unfinished saves**, discard interrupted uploads before
   saving anew. Unfinished saves are not openable files.

Ordinary File Save/Export remains a local download. Cloud saves are explicit. Browser recovery
and automatic saving are future work; do not close unsaved work. The panel explains current
limits and the access-control/privacy model. Internet Identity credentials are entered only
in the genuine II window; no password or private key is sent to the PhotoCraft backend.

## Mainnet demonstration

Live application: **[PhotoCraft on ICP](https://reyqh-myaaa-aaaas-amyeq-cai.icp.net/)**.
Deployed on 2026-10-07:

- Frontend: `reyqh-myaaa-aaaas-amyeq-cai`
- Cloud backend: `rn333-2qaaa-aaaas-amyfa-cai`
- Both canisters have the verified NNS web principal as their sole controller.

The deployment uses the NNS web account as controller, linked through a temporary Internet
Identity delegation. Use a fresh CLI identity name for each session, stored in the default
OS keyring. Sign in to the intended NNS account and verify its principal before deploying:

```sh
icp identity list
icp identity link web <session-name> --app nns.ic0.app
icp identity principal --identity <session-name>
icp cycles balance -n ic --identity <session-name>
```

The CLI delegation acts as the NNS principal; it is not an additional controller. Expiring
the delegation does not remove the NNS account's control. The user's PhotoCraft sign-in has
a different, app-specific principal, even when the same Internet Identity is used.

The selected European subnet is
`bkfrj-6k62g-dycql-7h53p-atvkj-zg4to-gaogh-netha-ptybj-ntsgw-rqe`.
It hosts both the frontend and cloud backend. For an initial deployment, fund the cycles
ledger first and then create each canister with 2T cycles (4T total, plus ledger fees):
Creation and installation costs are deducted from that allocation, so the resulting execution
balances are lower than 2T each.

```sh
RUSTUP_TOOLCHAIN=1.95.0 ICP_CLI_PLUGIN_COMPUTE_LIMIT_SECS=300 icp deploy -e ic \
  --identity <session-name> \
  --controller <verified-nns-principal> \
  --subnet bkfrj-6k62g-dycql-7h53p-atvkj-zg4to-gaogh-netha-ptybj-ntsgw-rqe \
  --cycles 2t
icp canister status -e ic --identity <session-name>
python3 packaging/icp/verify.py --environment ic
```

Always pass the linked identity explicitly; do not change the CLI's global default.
ICP-to-cycles conversion spends ICP and should use an explicitly approved amount. A funding
allocation is not a recurring subscription, and the initial amount is not a lifetime budget.

Use the certified `https://<frontend-canister-id>.icp.net` address printed by deployment.
Commit the generated public ID mappings under `.icp/data/`; never commit identity keys or
local `.icp/cache/`. Keep the application origin stable; a new canister/custom domain may
change the user's app-specific principal unless II alternative origins are configured.
Local test files do not migrate to mainnet automatically. Download a local `.pcraft` copy,
open it in the mainnet app, and use **File → Sovereign Cloud → Save** to save it there.

The service initially permits signed-in users to create owner-only files within the
published quotas. The global 2 GiB document cap bounds this demo but can be exhausted by
registrations. Choose admission controls and funding policy before advertising a public service.
Monitor canister cycles and maintain a backup controller. Document encryption, sharing,
billing and dedicated Cloud Engine deployment are not included in this milestone.

### Upgrades and backup

A normal deploy upgrades the Rust backend in place and preserves its stable structures.
After linking a fresh NNS CLI session, use `icp deploy -e ic --no-create --identity <session-name>`
for updates. `--no-create` prevents accidentally creating replacement canisters if the public
ID mappings are missing. The recorded canister IDs keep existing data and the app origin stable.
Never use `--mode reinstall` on `cloud` while retaining documents: reinstall clears storage.
Take and download canister snapshots before risky upgrades, and keep user-exportable `.pcraft`
backups. The versioned Candid contract is `icp/backend/cloud.did`; memory IDs are documented in
`icp/backend/src/store.rs`. Schema changes need migration and upgrade tests, not just compilation.

## Checks

```sh
cargo +1.95.0 fmt --manifest-path icp/Cargo.toml --all --check
cargo +1.95.0 test --manifest-path icp/Cargo.toml -p photocraft-cloud --lib --locked
cargo +1.95.0 clippy --manifest-path icp/Cargo.toml --workspace --all-targets --locked -- -D warnings
cargo +1.95.0 build --manifest-path icp/Cargo.toml -p photocraft-cloud --target wasm32-unknown-unknown --release --locked
PHOTOCRAFT_CLOUD_WASM="$PWD/icp/target/wasm32-unknown-unknown/release/photocraft_cloud.wasm" cargo +1.95.0 test --manifest-path icp/Cargo.toml -p photocraft-cloud --test persistence --locked
cargo +1.95.0 check -p photocraft-web --target wasm32-unknown-unknown --locked
cargo +1.95.0 clippy -p photocraft-web --features icp-cloud --target wasm32-unknown-unknown --no-deps --locked -- -D warnings
cargo +1.95.0 xtask layers
```

PocketIC creates isolated test instances. It downloads its version-matched server from its
upstream release when needed; set `POCKET_IC_SERVER_URL` to an existing PocketIC server to
reuse it. This is the PocketIC control port, not the HTTP gateway port. Tests cover upload
retry, atomic publication, ownership, quota/length/checksum rejection, upgrade persistence,
contract generation and the maximum document's commit budget.

After a backend interface change, regenerate the checked-in contract:

```sh
cargo +1.95.0 run --manifest-path icp/Cargo.toml -p photocraft-cloud --example export_candid > icp/backend/cloud.did
```

CI retains upstream tests and adds `.github/workflows/icp-cloud.yml`. The final browser
identity ceremony and cross-device save/open also need an interactive smoke test; compile
and backend tests alone do not validate passkey or popup behavior.

## HTTP and build behavior

Packaging adds a Wasm-compatible CSP with hashes of Trunk's generated bootstrap scripts,
MIME/cache settings, license notices and Internet Identity app metadata. The application
uses same-origin API requests; the II popup runs at its own origin. No handwritten JavaScript
or new UI framework is introduced. CPU work is currently single-threaded; adding shared-memory
workers will require its own compatibility testing, especially alongside popup sign-in.

The HTTP checker supports local and mainnet deployments. It compares the served HTML/JS/Wasm to the local build, checks certificate
headers, compression, MIME/CSP/cache settings, missing-file responses, the configured cloud
canister/root key and Internet Identity metadata. The HTTP gateway
verifies certification; this byte check is not a separate cryptographic security audit.
On mainnet it also requires the canonical IC root key, rather than accepting an arbitrary
local-network trust key. Run its regression checks with
`python3 -B -m unittest discover -s packaging/icp -p 'test_*.py'`.

## Validation record

Validated locally on 2026-10-06–07:

- Five backend unit/contract tests passed.
- An additional PocketIC authorization test passes against every file API: two principals and
  an anonymous caller, pending and committed uploads, mixed file/revision IDs, and checks
  before and after upgrade. Unauthorized calls leave the owner's bytes intact. This tests
  application authorization; it is not proof of end-to-end confidentiality.
- Two PocketIC integration tests passed: ownership, interrupted upload, exact-byte retrieval
  and repeated upgrades; plus a full 64 MiB / 128-chunk upload and commit within the default
  canister execution budget.
- Backend and optional-cloud Wasm clippy passed; the normal web build and all 27 dependency
  layers passed. Upstream's job tests passed (11 passed; one opt-in timing test ignored).
- Both canisters deployed to the managed local network. Certified asset byte/header checks,
  runtime environment and II metadata checks passed; the cloud panel rendered in the browser.
- The user confirmed Internet Identity sign-in and core browser cloud saving/loading work.
  This is an interactive user report, not an automated cross-device or reload test. The
  automated browser initially did not expose the II popup; the panel supports reopening
  sign-in without reloading the document.
- The File-menu follow-up passed 498 shared UI tests (three opt-in tests ignored), including
  command ordering, absent-document/disabled guards and rejection of automation requests.
  The native editor still builds with no cloud commands registered. All 21 Wasm-safe crates
  and 27 dependency-layer checks pass. The cloud adapter passes strict clippy. Full native UI
  clippy reports three pre-existing `collapsible_match` style warnings in unchanged upstream
  `analysis_ui.rs`/`canvas.rs`; it passes when only that lint is excluded.
- Visually verified the ICP logo, File-menu placement, disabled Save to Cloud without a
  document, enabled Save to Cloud after creating one, and both menu routes opening the cloud
  manager. The updated build's certified asset and runtime-configuration checks pass. The
  signed-in controls are additionally exercised with synthetic data in shared-widget tests.
- Eight cloud flow/presentation tests pass: unchanged-save routing, first-save/copy naming,
  file extensions, blocked saves while busy/invalid, mouse/keyboard row expansion, independent
  Open buttons, stable expansion after sorting, and file/progress layouts across all five
  themes at 420 px and 760 px widths. Launcher alignment is checked against the measured
  options bar across all themes, including changes to row height and hiding the toolbar.
  Thirty-five offscreen PNGs were rendered, including expanded history and launcher alignment; dark and light layouts
  were visually inspected for alignment, separation and long-name truncation.
  These test the actual browser widgets without bypassing Internet Identity. Strict native
  adapter and Wasm clippy, the ordinary web check and all 27 layer checks pass.
- The refined frontend was deployed to the existing PocketIC network and visually checked in
  a fresh browser tab. Certified assets/runtime configuration/II metadata pass; browser
  error logs are empty. Authenticated save/copy/retry on this exact build still needs the
  interactive account smoke test above.
- The File-menu-only follow-up also deployed successfully: save/copy actions are disabled
  without a document, enabled with a synthetic document, and the copy route opens the cloud
  sign-in flow. Certified served assets/configuration pass; no browser errors. The duplicate
  save controls are absent from the shared file/progress layouts.
- The compact-file-list build is deployed locally. A fresh browser tab confirmed the larger
  window, resizing and scrolling to overflow content; certified assets and runtime checks
  pass with no browser errors. Signed-in rows and history interactions were verified with
  synthetic native widget fixtures; the browser session was signed out.

Mainnet validation on 2026-10-07:

- Both canisters are running with only the approved NNS account as controller. Certified
  state reads through the official Rust agent independently confirmed both are on the
  European `bkfrj` subnet, using the pinned IC root key.
- The frontend's served HTML/JS/Wasm matches the tested build from `5b1763a`; the Wasm is
  26,226,607 bytes. Certified delivery, cache/MIME/CSP settings, the backend ID, canonical
  mainnet root key and Internet Identity metadata checks pass.
- The browser renders the editor and cloud sign-in panel without console errors. An
  authenticated CLI call returns the new account's empty file list. A complete browser
  sign-in/save/reload/open cycle on mainnet remains an interactive account smoke test;
  no synthetic files were added to the mainnet backend during deployment.
- The deployment verifier also passes locally, and its four trust/mapping regression
  tests are included in the ICP workflow.

The earlier delivery proof of concept rendered with WebGPU and forced WebGL2, imported an
image, painted and undid a stroke. PNG export reported success in the app, but automation did
not capture the downloaded file. No dedicated Cloud Engine test has been performed.
Treat this as an integration milestone; cross-device cloud validation remains outstanding.

Shared-account local validation on 2026-10-10: the root portal and PhotoCraft shared two
disposable signing identities on a separate test origin. Save, catalog listing, deep-link
reopen with painted content, and a second revision under the same file ID all passed.
Account changes cleared cloud lists, preserved open documents and blocked accidental saves
to a different account. Portal logout propagated without editor reload. Malformed delegation
storage recovered without panic. Browser error logs were empty. These are real signed local
canister calls; Internet Identity/passkey issuance and cross-device use were not automated.
Service tests (6), SDK tests (9), PocketIC persistence/upgrade tests (4), standalone backend
tests (6), host tests (8), strict changed-package clippy and all 27 layer checks pass.

The cloud UX refinement keeps all user-facing records named **files**. The legacy Candid
`Project` type and storage schema remain unchanged, preserving existing data and clients.
The optional browser adapter owns the blue/violet styling; native editor themes are unchanged.
CPU encoding still runs synchronously after the initial loading frame; large files can pause
spinner animation during encoding. A worker implementation remains future work.
