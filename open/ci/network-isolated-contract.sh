#!/usr/bin/env bash

set -euo pipefail

workspace_root="$(CDPATH= cd -- "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
workflow="$workspace_root/.github/workflows/network-isolated.yml"
cross_platform_workflow="$workspace_root/.github/workflows/ci.yml"
provisioner="$workspace_root/.github/scripts/provision-linux-libva.sh"
windows_libvpx_provisioner="$workspace_root/.github/scripts/provision-windows-libvpx.sh"
harness="$workspace_root/open/ci/run-network-isolated.sh"

fail() {
    printf 'network-isolated contract: %s\n' "$1" >&2
    exit 1
}

[[ -f "$workflow" ]] || fail "missing GitHub Actions workflow"
[[ -x "$harness" ]] || fail "missing executable isolation harness"
[[ -x "$provisioner" ]] || fail "missing executable Linux libva provisioner"
[[ -x "$windows_libvpx_provisioner" ]] || fail "missing executable Windows libvpx provisioner"

bash -n "$harness"
bash -n "$provisioner"
bash -n "$windows_libvpx_provisioner"

grep -Fq 'runs-on: ubuntu-24.04' "$workflow" || fail "workflow must use Ubuntu 24.04"
grep -Fq 'permissions:' "$workflow" || fail "workflow must declare permissions"
grep -Fq 'contents: read' "$workflow" || fail "workflow must use read-only repository access"
grep -Fq 'actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683' "$workflow" ||
    fail "workflow must pin the approved checkout revision"
grep -Fq 'persist-credentials: false' "$workflow" ||
    fail "workflow must disable persisted checkout credentials"
grep -Fq 'rustup toolchain install stable --profile minimal' "$workflow" ||
    fail "workflow must install the declared Rust toolchain"
grep -Fq '.github/scripts/provision-linux-libva.sh' "$workflow" ||
    fail "workflow must use the shared Linux libva provisioner"
[[ "$(grep -Fc '../.github/scripts/provision-linux-libva.sh' "$cross_platform_workflow")" -eq 2 ]] ||
    fail "both cross-platform Linux jobs must use the shared libva provisioner"
grep -Fq 'libva_version="2.22.0"' "$provisioner" ||
    fail "provisioner must pin libva 2.22.0"
grep -Fq 'libva_sha256="e3da2250654c8d52b3f59f8cb3f3d8e7fb1a2ee64378dbc400fbc5663de7edb8"' "$provisioner" ||
    fail "provisioner must pin the reviewed libva source digest"
grep -Fq 'sudo apt-get install --yes libdrm-dev libgbm-dev meson nasm ninja-build pkg-config' "$provisioner" ||
    fail "provisioner must install exact source-build prerequisites"
grep -Fq 'va/va_dec_vvc.h' "$provisioner" ||
    fail "provisioner must verify the required VVC header"
grep -Fq 'pkg-config --atleast-version=1.22.0 libva' "$provisioner" ||
    fail "provisioner must verify the required libva API version"
grep -Fq 'meson compile -C "$build" --jobs 2' "$provisioner" ||
    fail "provisioner must use the portable Meson compile build-directory option"
grep -Fq 'meson install -C "$build" --no-rebuild' "$provisioner" ||
    fail "provisioner must use the portable Meson install build-directory option"
! grep -Eq '^meson (compile|install).*--directory' "$provisioner" ||
    fail "provisioner must not use Meson build-directory syntax rejected by Ubuntu 24.04"
grep -Fq 'CROS_LIBVA_H_PATH=' "$provisioner" ||
    fail "provisioner must publish the reviewed header path"
grep -Fq 'LIBRARY_PATH=$prefix/lib${LIBRARY_PATH:+:$LIBRARY_PATH}' "$provisioner" ||
    fail "provisioner must publish the private libva native linker path"
grep -Fq 'LD_LIBRARY_PATH=$prefix/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}' "$provisioner" ||
    fail "provisioner must publish the private libva runtime linker path"
[[ "$(grep -Fc '../.github/scripts/provision-windows-libvpx.sh' "$cross_platform_workflow")" -eq 1 ]] ||
    fail "the Windows matrix lane must use the approved libvpx provisioner exactly once"
grep -Fq 'vcpkg_baseline="a0400024711b283056538ac19ced80b91a83c24c"' "$windows_libvpx_provisioner" ||
    fail "Windows libvpx provisioning must pin the reviewed vcpkg registry revision"
grep -Fq 'VCPKG_BINARY_SOURCES=clear' "$windows_libvpx_provisioner" ||
    fail "Windows libvpx provisioning must build from the pinned source package"
grep -Fq -- '--triplet x64-mingw-dynamic' "$windows_libvpx_provisioner" ||
    fail "Windows libvpx provisioning must produce a dynamically loadable runtime"
grep -Fq '"features": ["highbitdepth"]' "$windows_libvpx_provisioner" ||
    fail "Windows libvpx provisioning must retain VP9 high-bit-depth support"
grep -Fq 'SUPERI_LIBVPX_PATH=' "$windows_libvpx_provisioner" ||
    fail "Windows libvpx provisioning must publish the exact runtime path"
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
grep -Fq '/proc/net/dev' "$harness" ||
    fail "harness must inspect interfaces through the current network namespace"
! grep -Fq '/sys/class/net/' "$harness" ||
    fail "harness must not inspect interfaces through the host-mounted sysfs view"
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
