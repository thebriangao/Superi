#!/usr/bin/env bash

set -euo pipefail

: "${RUNNER_TEMP:?RUNNER_TEMP is required}"
: "${GITHUB_ENV:?GITHUB_ENV is required}"
: "${VCPKG_INSTALLATION_ROOT:?VCPKG_INSTALLATION_ROOT is required}"

vcpkg_baseline="a0400024711b283056538ac19ced80b91a83c24c"
manifest_root="$RUNNER_TEMP/superi-libvpx-manifest"
install_root="$RUNNER_TEMP/superi-libvpx-installed"
vcpkg_root="$(cygpath --unix "$VCPKG_INSTALLATION_ROOT")"

mkdir -p "$manifest_root"
cat > "$manifest_root/vcpkg.json" <<EOF
{
  "name": "superi-ci-libvpx",
  "version-string": "0",
  "builtin-baseline": "$vcpkg_baseline",
  "dependencies": [
    {
      "name": "libvpx",
      "features": ["highbitdepth"]
    }
  ]
}
EOF

VCPKG_BINARY_SOURCES=clear "$vcpkg_root/vcpkg.exe" install \
    --triplet x64-mingw-dynamic \
    --x-manifest-root="$manifest_root" \
    --x-install-root="$install_root" \
    --clean-after-build

mapfile -t runtimes < <(
    find "$install_root/x64-mingw-dynamic/bin" -maxdepth 1 -type f \
        \( -iname 'libvpx*.dll' -o -iname 'vpx*.dll' \)
)
[[ "${#runtimes[@]}" -eq 1 ]] || {
    printf 'expected one libvpx runtime, found %s\n' "${#runtimes[@]}" >&2
    exit 1
}

runtime="$(cygpath --windows "${runtimes[0]}")"
echo "SUPERI_LIBVPX_PATH=$runtime" >> "$GITHUB_ENV"
