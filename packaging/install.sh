#!/bin/sh
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 insideTheFlux
# Installs RadioFoundry CPS from the unpacked .tar.gz for the current user (no root needed), or for everyone with
#   PREFIX=/usr/local sudo ./install.sh
# Remove it again with: ./install.sh uninstall
set -eu
cd "$(dirname "$0")"
PREFIX=${PREFIX:-$HOME/.local}
BIN=$PREFIX/bin/rfcps
APP=$PREFIX/share/applications/rfcps.desktop
ICON=$PREFIX/share/icons/hicolor/256x256/apps/rfcps.png
DOC=$PREFIX/share/doc/rfcps

if [ "${1:-}" = uninstall ]; then
	rm -f "$BIN" "$APP" "$ICON"
	rm -rf "$DOC"
	echo "Removed RadioFoundry CPS from $PREFIX"
	exit 0
fi

[ -f rfcps ] || { echo "Run this from the unpacked rfcps folder (rfcps is missing here)." >&2; exit 1; }
install -Dm755 rfcps "$BIN"
install -Dm644 rfcps.desktop "$APP"
install -Dm644 rfcps.png "$ICON"
install -d "$DOC"
install -m644 LICENSE AUTHORS CONTRIBUTORS NOTICE.md THIRD_PARTY_LICENSES.txt "$DOC"/
command -v update-desktop-database > /dev/null && update-desktop-database "$PREFIX/share/applications" 2> /dev/null || true
command -v gtk-update-icon-cache > /dev/null && gtk-update-icon-cache -q "$PREFIX/share/icons/hicolor" 2> /dev/null || true
echo "Installed to $PREFIX. Start it from the application menu or with: rfcps"
case ":$PATH:" in *":$PREFIX/bin:"*) ;; *) echo "Note: $PREFIX/bin is not on your PATH; add it to run 'rfcps' from a terminal." ;; esac
id -nG | grep -qw dialout || echo "To use a serial port, add your user to the dialout group (sudo usermod -aG dialout \$USER) and log in again."
