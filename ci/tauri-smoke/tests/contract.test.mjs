import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const moduleRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(moduleRoot, "../..");
const tauriRoot = resolve(moduleRoot, "src-tauri");

test("Tauri smoke host pins a real native Tauri 2 build", () => {
  const manifest = readFileSync(resolve(tauriRoot, "Cargo.toml"), "utf8");
  const lockfile = readFileSync(resolve(tauriRoot, "Cargo.lock"), "utf8");
  const source = readFileSync(resolve(tauriRoot, "src/lib.rs"), "utf8");

  assert.match(manifest, /^tauri = \{ version = "=2\.11\.5",/m);
  assert.match(manifest, /^tauri-build = \{ version = "=2\.6\.3"/m);
  assert.match(lockfile, /name = "tauri"\nversion = "2\.11\.5"/);
  assert.match(source, /tauri::Builder::default\(\)/);
  assert.match(source, /tauri::test::mock_builder\(\)/);
});

test("workflow runs every Rust gate on each native desktop family", () => {
  const workflow = readFileSync(
    resolve(repositoryRoot, ".github/workflows/tauri.yml"),
    "utf8",
  );

  assert.match(workflow, /permissions:\n  contents: read/);
  assert.match(
    workflow,
    /actions\/checkout@11bd71901bbe5b1630ceea73d27597364c9af683/,
  );
  for (const runner of [
    "macos-26",
    "macos-15-intel",
    "windows-2025",
    "ubuntu-24.04",
  ]) {
    assert.match(workflow, new RegExp(`runner: ${runner.replace(".", "\\.")}`));
  }

  assert.match(workflow, /libwebkit2gtk-4\.1-dev/);
  assert.match(workflow, /libayatana-appindicator3-dev/);
  assert.match(workflow, /working-directory: ci\/tauri-smoke\/src-tauri/);
  assert.match(workflow, /run: cargo fmt -- --check/);
  assert.match(workflow, /run: cargo test --locked/);
  assert.match(
    workflow,
    /run: cargo clippy --all-targets --locked -- -D warnings/,
  );
  assert.match(
    workflow,
    /run: cargo build --locked --bin superi-tauri-smoke/,
  );
  assert.doesNotMatch(workflow, /continue-on-error|\|\| true|--if-present/);
});
