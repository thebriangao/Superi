# Superi: Open Tree (MIT)

The free, forkable, offline-complete professional editor. This cargo workspace is the **engine**:
headless, scriptable, and fully functional with the network
unplugged.

> **Status: structural skeleton.** Crates compile and map the architecture; engine logic, CI rails,
> the vertical slice, and the UI are later passes (see `docs/STRUCTURE.md` § "Deferred").

## Build

```bash
cd open
cargo build
cargo run -p superi-cli      # prints the scaffold version line
```

Enable the opt-in OS codec backend (H.264/H.265/ProRes/AAC via the user's OS, see
`../docs/codecs.md`):

```bash
cargo build -p superi-cli --features os-codecs
```

## Layout

18 crates in `crates/`, one per `§5` subsystem, wired in strict downward-only dependency tiers so the
architecture is compiler-enforced. Full crate map, dependency DAG, ownership, and the workspace guide are
in **`docs/STRUCTURE.md`**.

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
