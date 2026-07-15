#!/usr/bin/env bash

set -euo pipefail

: "${RUNNER_TEMP:?RUNNER_TEMP is required}"
: "${GITHUB_ENV:?GITHUB_ENV is required}"

libva_version="2.22.0"
libva_sha256="e3da2250654c8d52b3f59f8cb3f3d8e7fb1a2ee64378dbc400fbc5663de7edb8"
archive="$RUNNER_TEMP/libva-$libva_version.tar.bz2"
source="$RUNNER_TEMP/libva-$libva_version"
build="$RUNNER_TEMP/libva-$libva_version-build"
prefix="$RUNNER_TEMP/libva-$libva_version-install"

sudo apt-get update
sudo apt-get install --yes libdrm-dev meson nasm ninja-build pkg-config

curl --fail --location --silent --show-error \
    "https://github.com/intel/libva/releases/download/$libva_version/libva-$libva_version.tar.bz2" \
    --output "$archive"
echo "$libva_sha256  $archive" | shasum --algorithm 256 --check
tar --extract --bzip2 --file "$archive" --directory "$RUNNER_TEMP"

meson setup "$build" "$source" \
    --prefix "$prefix" \
    --libdir lib \
    --buildtype release \
    -Dwith_x11=no \
    -Dwith_glx=no \
    -Dwith_wayland=no
meson compile -C "$build" --jobs 2
meson install -C "$build" --no-rebuild

test -f "$prefix/include/va/va_dec_vvc.h"
PKG_CONFIG_PATH="$prefix/lib/pkgconfig" pkg-config --atleast-version=2.22.0 libva

{
    echo "CROS_LIBVA_H_PATH=$prefix/include"
    echo "PKG_CONFIG_PATH=$prefix/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
    echo "LD_LIBRARY_PATH=$prefix/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
} >> "$GITHUB_ENV"
