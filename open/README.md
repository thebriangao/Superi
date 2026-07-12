# Galileo: Open Tree (MIT)

The free, forkable, offline-complete professional editor. This cargo workspace is the **engine**:
headless, scriptable, and fully functional with the network
unplugged.

> **Status: structural skeleton.** Crates compile and map the architecture; engine logic, CI rails,
> the vertical slice, and the UI are later passes (see `docs/STRUCTURE.md` § "Deferred").

## Build

```bash
cd open
cargo build
cargo run -p galileo-cli      # prints the scaffold version line
```

Enable the opt-in OS codec backend (H.264/H.265/ProRes/AAC via the user's OS, see
`../docs/codecs.md`):

```bash
cargo build -p galileo-cli --features os-codecs
```

## Layout

18 crates in `crates/`, one per `§5` subsystem, wired in strict downward-only dependency tiers so the
architecture is compiler-enforced. Full crate map, dependency DAG, ownership, and the workspace guide are
in **`docs/STRUCTURE.md`**.

## The rules that govern this tree

- **Offline law** (`../docs/architecture.md`): no network for core functionality, ever.
- **License policy** (`../docs/codecs.md`, `deny.toml`): permissive only, zero copyleft.
- **One-way boundary**: this tree never imports the `closed/` tree.
