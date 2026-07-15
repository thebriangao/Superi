#!/usr/bin/env bash

set -euo pipefail

fail() {
    printf 'network isolation failed: %s\n' "$1" >&2
    exit 1
}

[[ "$(uname -s)" == "Linux" ]] || fail "the harness requires Linux"
[[ -n "${SUPERI_HOST_NETNS:-}" ]] || fail "the host network namespace was not recorded"

current_netns="$(readlink /proc/self/ns/net)"
[[ "$current_netns" != "$SUPERI_HOST_NETNS" ]] ||
    fail "the workflow did not enter a distinct network namespace"

mapfile -t interfaces < <(
    awk -F: 'NR > 2 { name = $1; gsub(/^[[:space:]]+|[[:space:]]+$/, "", name); print name }' \
        /proc/net/dev
)
(( ${#interfaces[@]} > 0 )) || fail "the isolated namespace exposes no loopback interface"
for interface in "${interfaces[@]}"; do
    [[ "$interface" == "lo" ]] || fail "unexpected network interface $interface is available"
done

if tail -n +2 /proc/net/route | grep -q '[^[:space:]]'; then
    fail "the isolated namespace has an IPv4 route"
fi

if timeout 2 bash -c 'exec 3<>/dev/tcp/1.1.1.1/53' 2>/dev/null; then
    fail "an outbound numeric socket unexpectedly connected"
fi

export CARGO_NET_OFFLINE=true

CDPATH= cd -- "$(dirname "${BASH_SOURCE[0]}")/.."

printf 'network namespace %s has no outbound interface or route\n' "$current_netns"
cargo test --workspace --locked --offline
cargo run --locked --offline -p superi-fixture-tool -- check test-fixtures
slice_root="$(mktemp -d)"
trap 'rm -rf "$slice_root"' EXIT
cargo run --locked --offline -p superi-cli -- slice run \
    --scenario superi.slice.canonical.v1 \
    --artifact-dir "$slice_root/artifacts" \
    --report "$slice_root/report.json"
