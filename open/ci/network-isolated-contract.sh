#!/usr/bin/env bash

set -euo pipefail

workspace_root="$(CDPATH= cd -- "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
workflow="$workspace_root/.github/workflows/network-isolated.yml"
harness="$workspace_root/open/ci/run-network-isolated.sh"

fail() {
    printf 'network-isolated contract: %s\n' "$1" >&2
    exit 1
}

[[ -f "$workflow" ]] || fail "missing GitHub Actions workflow"
[[ -x "$harness" ]] || fail "missing executable isolation harness"

bash -n "$harness"

grep -Fq 'runs-on: ubuntu-24.04' "$workflow" || fail "workflow must use Ubuntu 24.04"
grep -Fq 'permissions:' "$workflow" || fail "workflow must declare permissions"
grep -Fq 'contents: read' "$workflow" || fail "workflow must use read-only repository access"
grep -Fq 'actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683' "$workflow" ||
    fail "workflow must pin the approved checkout revision"
grep -Fq 'persist-credentials: false' "$workflow" ||
    fail "workflow must disable persisted checkout credentials"
grep -Fq 'rustup toolchain install stable --profile minimal' "$workflow" ||
    fail "workflow must install the declared Rust toolchain"
grep -Fq 'sudo apt-get install --yes libva-dev nasm' "$workflow" ||
    fail "workflow must install the approved Linux media build dependencies"
grep -Fq 'LIBVPX_VERSION: "1.16.0"' "$workflow" ||
    fail "workflow must pin the approved libvpx version"
grep -Fq 'LIBVPX_SOURCE_SHA256:' "$workflow" ||
    fail "workflow must pin the approved libvpx source digest"
grep -Fq 'shasum --algorithm 256 --check' "$workflow" ||
    fail "workflow must verify the libvpx source digest"
grep -Fq 'SUPERI_LIBVPX_PATH=$SUPERI_LIBVPX_PATH' "$workflow" ||
    fail "workflow must transfer the approved libvpx path into isolation"
grep -Fq 'timeout-minutes:' "$workflow" || fail "workflow must have a timeout"
grep -Fq 'cancel-in-progress: true' "$workflow" || fail "workflow must cancel superseded work"
grep -Fq 'unshare --net' "$workflow" || fail "workflow must enter a network namespace"
grep -Fq 'cargo test --workspace --locked --no-run' "$workflow" ||
    fail "workflow must prepare locked test artifacts before isolation"

grep -Fq 'CARGO_NET_OFFLINE=true' "$harness" || fail "harness must force Cargo offline"
grep -Fq 'cargo test --workspace --locked --offline' "$harness" ||
    fail "harness must run locked offline workspace tests"
grep -Fq 'superi-fixture-tool -- check test-fixtures' "$harness" ||
    fail "harness must validate canonical fixtures"
grep -Fq 'cargo run --locked --offline -p superi-cli -- slice run' "$harness" ||
    fail "harness must run the canonical headless slice"
grep -Fq -- '--scenario superi.slice.canonical.v1' "$harness" ||
    fail "harness must select the canonical scenario"
grep -Fq -- '--artifact-dir "$slice_root/artifacts"' "$harness" ||
    fail "harness must isolate canonical slice artifacts"
grep -Fq -- '--report "$slice_root/report.json"' "$harness" ||
    fail "harness must retain the canonical report"

printf 'network-isolated workflow contract passed\n'
