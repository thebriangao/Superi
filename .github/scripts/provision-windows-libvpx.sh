#!/usr/bin/env bash

set -euo pipefail

: "${RUNNER_TEMP:?RUNNER_TEMP is required}"
: "${GITHUB_ENV:?GITHUB_ENV is required}"
: "${VCPKG_INSTALLATION_ROOT:?VCPKG_INSTALLATION_ROOT is required}"

vcpkg_baseline="a0400024711b283056538ac19ced80b91a83c24c"
script_root="$(CDPATH= cd -- "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
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
    --triplet x64-windows-static \
    --x-manifest-root="$manifest_root" \
    --x-install-root="$install_root" \
    --clean-after-build

mapfile -t archives < <(
    find "$install_root/x64-windows-static/lib" -maxdepth 1 -type f \
        -iname '*vpx*.lib'
)
[[ "${#archives[@]}" -eq 1 ]] || {
    printf 'expected one libvpx archive, found %s\n' "${#archives[@]}" >&2
    exit 1
}

runtime="$RUNNER_TEMP/libvpx.dll"
exports_log="$RUNNER_TEMP/libvpx-exports.txt"
vs_install="$(vswhere.exe -latest -products '*' \
    -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 \
    -property installationPath | tr -d '\r')"
[[ -n "$vs_install" ]] || {
    printf 'Visual Studio C++ tools were not found\n' >&2
    exit 1
}

archive_windows="$(cygpath --windows "${archives[0]}")"
definition_windows="$(cygpath --windows "$script_root/libvpx-windows.def")"
runtime_windows="$(cygpath --windows "$runtime")"
exports_log_windows="$(cygpath --windows "$exports_log")"
dev_shell="$vs_install/Common7/Tools/Launch-VsDevShell.ps1"

MSYS_NO_PATHCONV=1 powershell.exe -NoLogo -NoProfile -NonInteractive -Command \
    "& '$dev_shell' -Arch amd64 -HostArch amd64 -SkipAutomaticLocation; \
     & link.exe /NOLOGO /DLL '/WHOLEARCHIVE:$archive_windows' \
       '/DEF:$definition_windows' '/OUT:$runtime_windows'; \
     if (\$LASTEXITCODE -ne 0) { exit \$LASTEXITCODE }; \
     & dumpbin.exe /NOLOGO /EXPORTS '$runtime_windows' | \
       Out-File -FilePath '$exports_log_windows' -Encoding ascii; \
     if (\$LASTEXITCODE -ne 0) { exit \$LASTEXITCODE }"

while read -r symbol; do
    symbol="${symbol%$'\r'}"
    [[ -z "$symbol" || "$symbol" == LIBRARY* || "$symbol" == EXPORTS ]] && continue
    grep -Fq "$symbol" "$exports_log" || {
        printf 'required libvpx export missing: %s\n' "$symbol" >&2
        exit 1
    }
done < "$script_root/libvpx-windows.def"

SUPERI_LIBVPX_PATH="$runtime_windows" python3 - <<'PY'
import ctypes
import os

runtime = ctypes.CDLL(os.environ["SUPERI_LIBVPX_PATH"])
runtime.vpx_codec_version_str.restype = ctypes.c_char_p
version = runtime.vpx_codec_version_str()
if version is None or not version.startswith(b"v1.16."):
    raise SystemExit(f"unexpected libvpx runtime version: {version!r}")
PY

echo "SUPERI_LIBVPX_PATH=$runtime_windows" >> "$GITHUB_ENV"
