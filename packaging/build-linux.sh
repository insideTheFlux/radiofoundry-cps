#!/bin/sh
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 insideTheFlux
# Builds the Linux packages into dist/: a .tar.gz (binary, desktop entry, icon, license) and a .deb for Debian and Ubuntu.
#   packaging/build-linux.sh
set -eu
cd "$(dirname "$0")/.."
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
ARCH=x86_64
cargo build --release -p rfcps
rm -rf dist/stage dist/rfcps-"$VERSION"-linux-"$ARCH".tar.gz
mkdir -p dist/stage/rfcps-"$VERSION"
cp target/release/rfcps packaging/rfcps.desktop packaging/rfcps.png LICENSE NOTICE.md README.md dist/stage/rfcps-"$VERSION"/
tar -C dist/stage -czf dist/rfcps-"$VERSION"-linux-"$ARCH".tar.gz rfcps-"$VERSION"
echo "dist/rfcps-$VERSION-linux-$ARCH.tar.gz"

if command -v dpkg-deb > /dev/null; then
	D=dist/stage/deb
	rm -rf "$D"
	mkdir -p "$D/DEBIAN" "$D/usr/bin" "$D/usr/share/applications" "$D/usr/share/icons/hicolor/256x256/apps" "$D/usr/share/doc/rfcps"
	install -m 755 target/release/rfcps "$D/usr/bin/rfcps"
	install -m 644 packaging/rfcps.desktop "$D/usr/share/applications/rfcps.desktop"
	install -m 644 packaging/rfcps.png "$D/usr/share/icons/hicolor/256x256/apps/rfcps.png"
	install -m 644 LICENSE NOTICE.md "$D/usr/share/doc/rfcps/"
	SIZE=$(du -sk "$D/usr" | cut -f1)
	cat > "$D/DEBIAN/control" <<CONTROL
Package: rfcps
Version: $VERSION
Section: hamradio
Priority: optional
Architecture: amd64
Installed-Size: $SIZE
Depends: libc6 (>= 2.31)
Recommends: libgl1, libxkbcommon0
Maintainer: insideTheFlux <5977335+insideTheFlux@users.noreply.github.com>
Description: RadioFoundry codeplug editor
 Reads, edits and writes the codeplug of a RadioFoundry radio over its USB or
 programming cable. To use a serial port, add your user to the "dialout" group
 and log in again.
CONTROL
	fakeroot dpkg-deb --build "$D" "dist/rfcps_${VERSION}_amd64.deb" > /dev/null
	echo "dist/rfcps_${VERSION}_amd64.deb"
fi
rm -rf dist/stage
