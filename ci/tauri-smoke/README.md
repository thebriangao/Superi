# Tauri Rust CI smoke host

This directory is an executable CI contract for the locked Tauri 2 desktop boundary. It is not the
Superi application and contains no editor state or behavior. The production shell remains Phase 3
work and will replace this smoke host as soon as its real Rust crate exists.

The host deliberately exercises two different boundaries:

- `cargo test` builds the command surface with Tauri's mock runtime, so unit tests require no
  display server or native webview process.
- `cargo build` compiles the same builder through Tauri's native `wry` runtime on macOS, Windows,
  and Linux. The CI binary constructs the builder without opening a window. This catches target SDK,
  linker, WebKitGTK, WebView2, and platform API regressions without pretending to be the Phase 3
  application.

Run the local gates from the repository root:

```text
cargo fmt --manifest-path ci/tauri-smoke/src-tauri/Cargo.toml -- --check
cargo test --manifest-path ci/tauri-smoke/src-tauri/Cargo.toml --locked
cargo clippy --manifest-path ci/tauri-smoke/src-tauri/Cargo.toml --all-targets --locked -- -D warnings
cargo build --manifest-path ci/tauri-smoke/src-tauri/Cargo.toml --locked --bin superi-tauri-smoke
node --test ci/tauri-smoke/tests/contract.test.mjs
```

Linux contributors need the Tauri development packages listed in
`.github/workflows/tauri.yml`, including WebKitGTK 4.1.
