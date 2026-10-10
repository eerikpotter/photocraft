# PhotoCraft × ICP Cloud

> Open source opened the code. The next step is to open the cloud.

This independent fork evolves [PhotoCraft](https://github.com/storytold/photocraft), by the
ArtCraft team and community, toward an open-source, self-hostable cloud service on ICP.
“Open SaaS” describes the service model, not a dependency on a particular SaaS framework.
The complete application backend should be deployable by a fork operator without an account
on the original operator's server, database or object store.

**Local compute. Network-native state. Sovereign cloud.**

## Current architecture

```text
User's browser                         Internet Computer
-----------------------------------    -------------------------------------
Rust / Wasm editor and egui UI    <---  Certified static-site frontend canister
Local CPU / WebGPU / WebGL2
Documents, filters, painting, undo

Internet Identity client          <->  Internet Identity (id.ai)
Explicit cloud save / open        <->  PhotoCraft cloud canister
                                       Ownership and revision manifests
                                       Chunked .pcraft data in stable memory
```

The editor Wasm is downloaded as an asset and executes on the user's device. It is not
installed as a graphics-processing canister. The cloud canister does not import the
PhotoCraft editor, execute filters or use external storage services.

Implementation boundaries:

- `vendor/sovereign-cloud`: checked snapshot of the internal, editor-independent Rust SDK.
  Its canonical source is `packages/sovereign-cloud` in the parent subnet-cloud integration
  repo. The SDK handles endpoint/trust configuration, identity adaptation, upload checkpoints,
  progress and verified downloads. The snapshot keeps this fork independently buildable.
- `icp/backend`: independent Rust canister workspace; durable state and access checks.
- `icp/protocol`: compatibility facade for the SDK's unchanged V1 Candid/serde types;
  independent of the editor document model. Existing method names and stable state are preserved.
- `apps/photocraft-web/src/cloud`: optional `icp-cloud` platform adapter and cloud panel.
- `crates/ui-egui/src/service_commands.rs`: provider-neutral host menu requests, empty by
  default. File-menu entries queue requests into the browser adapter; no ICP types or clients
  enter the shared editor. Native builds register no host commands.
- `packaging/icp`, `icp.yaml`: ICP build, certified delivery and deployment.
- Upstream `crates/*`: editing, rendering, codecs, UI and the ordinary `.pcraft` format.

Cloud operations are browser platform services, like file dialogs and downloads. They do
not add identity or networking dependencies to the engine. Document edits still use the
existing command system. A future agent API needs a deliberate authorization boundary;
the cloud panel is not an agent interface.

The subnet-cloud local workspace serves this app at `/apps/photocraft/` alongside three
independent Craft web builds from one frontend canister. Only PhotoCraft enables the SDK
in this milestone. The editor adapter still owns `.pcraft` export/import, immutable document
snapshots, document-to-cloud bindings and the existing cloud UI. File transfer and identity
mechanics live in the SDK. The root's new local network is separate from existing deployments;
no mainnet origin, account or data migration is performed by this refactor.

**File → Save to Cloud…** asks for a name on the first save and saves a new
version of an already linked file when the document has changed. **File → Open from Cloud…** opens **My files**.
The separate Sovereign Cloud window owns sign-in, file management and dated version history;
ordinary File Save/Export stays local. Host requests are visible in menu inspection and
explicitly reject automation until a capability model exists. They never replace built-in
editor commands. The generic menu extension is kept in its own commit for upstream review.

The cloud manager has a scoped blue/violet identity and a **My files** library. Saving starts
only from the File menu. **Save to Cloud…** asks for a name once, then uploads changes directly;
**Save a Copy to Cloud…** explicitly starts a new naming flow. Once uploading starts, the manager
shows progress and files, with no second save button. Repeating Save to Cloud at the same local
document revision does not create another version. The cloud snapshot revision is tracked
separately from ordinary local downloads, so a local save cannot suppress a needed cloud save.
The launcher centers vertically in the editor's measured options bar, following its theme
and size without modifying upstream toolbar code. Hiding that bar also hides the launcher;
the File menu remains the entry point when the toolbar is hidden.

Each file occupies one bordered row with its name, latest local save date/time and **Open file**.
Clicking the name/date area expands dated version history; opening a file is a separate action.
Long names truncate with the full name available on hover. Expansion follows the file ID through
refreshes and reordering. The wider, resizable cloud window owns the only scrollbar, so the list
and expanded history use its available space without a nested, fixed-height viewport.

Transfer progress and expiring activity notifications
are separate from files; incomplete records appear only under **Unfinished saves**. Encoding
captures the immutable document at click time and waits for a loading frame before running.
Only the committed snapshot is marked saved; subsequent edits remain dirty.

The public protocol still calls its single-file record `Project`. That internal name is kept
for Candid and stable-memory compatibility; it is not a workspace or folder. Actual projects,
folders and shared workspaces can be a future layer grouping these files.

## First cloud milestone

Implemented behavior, subject to the validation record in [icp.md](icp.md):

- Internet Identity sign-in with app-specific principals; no anonymous cloud storage.
- Owner-only files, explicit save/open, and a list of immutable revisions.
- Standard `.pcraft` snapshots in 512 KiB chunks; SHA-256 checked before publication and
  after download. Backend manifests come from consensus update calls; chunk queries are
  verified against those manifests.
- Expected-revision checks reject stale saves. Only one pending upload per file.
- Repeated identical chunks and repeated commits are idempotent. The browser can retry a
  failed save while its snapshot remains in memory. After a page reload an unfinished
  upload can be discarded; automatic cross-reload upload resumption is future work.
- A revision becomes visible only after every chunk passes the checksum. Uploading does
  not mark a document saved; the matching local snapshot is marked saved after commit.
  Edits made during upload stay dirty.
- All file metadata, pending uploads, quotas and bytes use stable structures. No bulk
  heap serialization is required during upgrades. Memory IDs in `store.rs` are permanent
  schema assignments and must never be repurposed.
- Owners can delete old revisions, discard pending uploads and remove empty file records.

Demo limits: 64 MiB per encoded document, 256 MiB per principal including versions and
reserved uploads, 20 files per principal, 20 versions per file, 1,000 files and
2 GiB of document data across the service. These are explicit prototype bounds, not ICP
protocol limits or a promise of production scale. Metadata and canister overhead add to
document storage. Open registration can exhaust the shared demo quota; operators should
choose admission and funding policies before a wider launch.

Local browser recovery is not yet implemented. Cloud saving is explicit; closing or
refreshing an unsaved document can lose work. CPU encoding/decoding and many filters still
run synchronously on Wasm. The new upstream job API is the intended seam for worker support.

## Identity, trust and sovereignty

The optional browser feature uses the Rust `ic-auth-client` port and `ic-agent`, pinned in
Cargo.lock. It implements II's documented `authorize-client` delegation exchange through
`https://id.ai/#authorize`; this is distinct from the newer JavaScript SDK's rotating-session
API. Delegations last at most eight hours here; expired sessions require sign-in again.
Idle handling must never reload the editor and discard unsaved work.

The frontend reads backend IDs and the network root key from the static-site `ic_env`
cookie. It never fetches and trusts a replacement root key from the replica. Identity is
origin-specific: decide the canonical production origin before onboarding users and follow
II's alternative-origin protocol when adding a custom domain. Moving to a new canister or
infrastructure does not automatically migrate user identities or data.

Ownership uses the authenticated IC caller, never an owner supplied in an API parameter.
`inspect_message` is only a cost optimization; each method independently checks access.
The backend makes no external or inter-canister calls and does not attach cycles.

“Sovereign” here means an open, deployable application and state layer using the ICP cloud
protocol. It does not imply that the user owns all physical nodes, that data is in a chosen
jurisdiction, that the operator cannot upgrade code, or that documents are encrypted from
node operators. This milestone provides access control, **not end-to-end encryption**.
The canister controllers retain upgrade authority. Publish the controller/governance model,
network/subnet, operator and data-location assumptions for each actual deployment.

The stronger requirement is that only the owner and explicitly authorized people or agents
may obtain file contents. Current application-level isolation is covered by an endpoint
regression test with two principals and an anonymous caller, before and after upgrade.
It does **not** prove confidentiality against controllers, compromised clients or node operators.
See ICP's [security model](https://docs.internetcomputer.org/concepts/security/) and
[controller model](https://docs.internetcomputer.org/concepts/canisters/#controllers).

Before claiming that stronger confidentiality, add client-side authenticated encryption for
file contents and sensitive metadata, recipient-controlled key access and recovery, and a
reviewed trust model for both frontend and backend upgrades. Encryption alone cannot defeat
a malicious replacement frontend or a replaceable key-authorization service. Threshold keys
such as [vetKeys](https://docs.internetcomputer.org/concepts/vetkeys/) are a candidate, not
an automatic solution to controller trust.

Future sharing must deny access by default, check per-file read/write/manage permissions on
every API call, and keep agent identities separate from the owner's identity. Agent grants
must be explicit, narrowly scoped, expiring and revocable; neither a recipient nor an agent
may increase its own privileges. Record grants, revocations and changes. Revocation prevents
future authorized access; it cannot erase plaintext or keys already copied by a prior recipient.
PocketIC is a development/test environment, not a production confidentiality boundary.

Public ICP and dedicated Cloud Engines share the canister model. Dedicated deployment is
a target, not a tested guarantee in this milestone. Validate the engine's gateway, identity
trust, API support and resource/funding model. Future threshold-key features may need an
engine proxy; do not assume cycle-bearing cross-subnet calls behave identically.

## Development phases

1. **Application delivery:** certified frontend hosting; browser CPU/GPU rendering.
2. **Cloud persistence:** the initial sign-in/save/open/revisions milestone. Follow with local
   recovery, robust browser jobs, cloud preferences, cost measurements and admission controls.
3. **Network-native document state:** keep snapshot and cloud-envelope versions separate;
   measure content-addressed asset and tile reuse before changing storage. Add restore/fork
   operations. Do not fork the upstream document format to carry account metadata.
4. **Sharing and collaboration:** owner/editor/viewer permissions, attribution and a defined
   concurrency model. Snapshot uploads with revision conflicts are not simultaneous editing.
5. **Agent authorization:** capability-scoped principals, permitted commands/layers, bounded
   resources, branch revisions and human approval before publication. Record actor, engine
   version, inputs/assets, randomness and result hashes. A command log alone cannot guarantee
   identical replay across engine versions, fonts, codecs or GPU implementations.
6. **Dedicated sovereign deployment:** validate portability, backup/restore and identity
   migration. Document node owners, operators, geography, jurisdiction, replication,
   controllers, upgrade governance and data ownership for the selected environment.

There is no dependency on upstream reaching Photoshop parity by a particular date. Integrate
its improvements continuously and validate complete workflows against representative files.

## Upstream integration

`origin` is `eerikpotter/photocraft`; `upstream` is `storytold/photocraft`.

1. Keep cloud and general editor changes in separate, reviewable commits.
2. Fetch upstream and merge `upstream/main` into an integration branch based on the fork.
   Preserve ancestry; do not repeatedly squash or cherry-pick the entire upstream history.
3. Keep upstream CI, corpus floors and normal native/web builds. Add optional-cloud Wasm
   checks, backend tests, PocketIC upgrades and a real browser save/open smoke test.
4. Review changed host interfaces and file-format versions, resolve conflicts, then publish
   a tested commit. Deploy pinned commits, not an automatically moving upstream head.
5. Offer general browser timing, persistence and worker improvements upstream. Keep ICP
   implementations, quotas and service policy in this fork.

The hosting baseline was committed separately before merging upstream's background-job
commits through `909efc0f6df0a684d270019384e355c850642e42`. The integration requires only
small feature-gated hooks in the web entry point and an optional generic UI service extension;
the cloud service has its own lockfile.
Internal API changes can still require adaptation. Maintaining a fork is not conflict-free.

## External dependencies and portability

Runtime: ICP gateways and consensus, Internet Identity, and the user's browser/device.
There is no PhotoCraft-owned AWS/Azure/GCP backend, SQL service, S3 bucket or third-party
authentication database. Build-time dependencies include Rust crates, Trunk, ICP tooling
and the certified-assets recipe. Rust dependency licenses and upstream asset notices remain
in force. The fork removes ArtCraft logo assets and credits the original project in text.

Model providers may be added for optional external AI capabilities. They must not become a
hidden dependency for ordinary save/open, file ownership or editing.

Relevant primary references: [II protocol](https://github.com/dfinity/internet-identity/blob/main/docs/ii-spec.mdx),
[stable structures](https://docs.internetcomputer.org/languages/rust/stable-structures/),
[ICP resource limits](https://docs.internetcomputer.org/references/resource-limits/),
[Cloud Engines](https://internetcomputer.org/wiki/cloud-engines/).
