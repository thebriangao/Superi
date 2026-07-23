# Superi: Open Tree (MIT)

The free, forkable, offline-complete professional editor. This cargo workspace is the **engine**:
headless, scriptable, and fully functional with the network
unplugged.

> **Status: active implementation.** Lower-level runtime contracts and CI rails are substantive, the
> canonical CLI slice executes at disclosed contract conformance, and the native retained interface
> foundation presents through wgpu with deterministic private capture. Production media import,
> graph evaluation, color delivery, muxing, playback, and the complete editor remain incomplete.

## Build

```bash
cd open
cargo build
cargo run -p superi-cli -- --help
cargo run -p superi-api-bindings -- check
cargo run -p superi-desktop
```

Run the fixed canonical slice from `open/` with absent output paths:

```bash
cargo run -p superi-cli -- slice run \
  --scenario superi.slice.canonical.v1 \
  --artifact-dir /tmp/superi-slice-artifacts \
  --report /tmp/superi-slice-report.json
```

Current success means `contract` conformance. The report names all six stub stages, and the
published `canonical.webm.contract-stub` is deliberately non-playable. Runtime conformance and a
real `canonical.webm` remain future integration work.

Enable the opt-in OS codec backend (H.264/H.265/ProRes/AAC via the user's OS, see
`../docs/codecs.md`):

```bash
cargo build -p superi-cli --features os-codecs
```

## Layout

Twenty-two runtime crates in `crates/` and seven repository tools in `tools/` are wired in strict
downward-only dependency tiers so the architecture is compiler-enforced. Full module maps,
dependency DAG, ownership, and workspace guidance are in **`../docs/codebase-map/`** and
**`docs/STRUCTURE.md`**.

The canonical public API registry also drives the committed desktop-facing TypeScript artifact at
`bindings/typescript/superi-api.ts`. Regenerate it with `superi-api-bindings generate`; use the
nonmutating `check` command in verification.

`superi-ui` owns the retained scene, Inter typography, original icon geometry, input, focus,
AccessKit semantics, deterministic paint, and shared wgpu compositor. `superi-session` owns portable
application and legacy-state behavior. `superi-desktop` is the thin native host, while
`superi-ui-inspect` and `../tools/superi-capture` control the same scene offscreen for private visual
evidence.

The supported local `superi-json` runtime accepts bounded `.superi-script.json` source through
`superi.project.script.run`. It executes the existing generic project command and complete editor
state query through the one public editor owner, with exact-source integrity, permission preflight,
conflict visibility, typed traces, and normal project durability. See
`../docs/checkpoints/P2.W07.C022.md` for the language, bounds, failure semantics, and complete source
example.

Successful generic project commands also append one bounded durable command record. Inspect the log
through `superi.project.command_log.get`, the matching `superi-json` step, local JSON-RPC automation,
or `superi-cli project command-log`; replayable detail reauthorizes every retained original request.
See `../docs/checkpoints/P2.W07.C025.md` for atomicity, persistence, retention, cursor, and recovery
guarantees.

## The rules that govern this tree

- **Offline law** (`../docs/architecture.md`): no network for core functionality, ever.
- **License policy** (`../docs/codecs.md`, `deny.toml`): permissive only, zero copyleft.
- **One-way boundary**: this tree never imports the `closed/` tree.

The workspace test gate enforces the offline and one-way boundaries with a static Cargo and Rust
scan. Run it directly before submitting dependency, source, or build-script changes:

```bash
cargo run -p superi-boundary-tool -- check .
```

The scanner rejects known HTTP, WebSocket, QUIC, and RPC client packages, including renamed Cargo
dependencies; direct socket APIs from the Rust standard library and common async runtimes; Cargo
paths or package identities that reference the closed product; Rust module paths and include macros
that cross into `closed/`; and symlinks that could escape the scanned tree. It reads without
following links or fetching dependencies, ignores inert comments and strings, emits stable
path-and-line diagnostics, and is exercised against the canonical tree by `cargo test --workspace`.
The cross-platform workflow also runs the locked command explicitly before every open workspace
build, and a scanner contract prevents those CI steps from drifting apart.

Core crates have no exceptions. A future user-installed plugin network capability requires an
explicit, narrowly scoped policy change that preserves a completely offline core editor; it must
not be hidden behind an allowlist entry in application code.
