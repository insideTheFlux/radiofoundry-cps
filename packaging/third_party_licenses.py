#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 insideTheFlux
"""Writes THIRD_PARTY_LICENSES.txt: every library the program is built from, its license, and its license texts.

    python3 packaging/third_party_licenses.py [OUT [TARGET_TRIPLE]]   (default: dist/THIRD_PARTY_LICENSES.txt, this machine)

It reads `cargo metadata` for the workspace, takes every dependency from a registry (the workspace's own crates are this
project) that the program is built from for the target (say x86_64-pc-windows-msvc), and copies the LICENSE, COPYING and NOTICE files that come with each crate's source. A crate
that ships none is listed with its license expression and its repository, so the notice can still be found. Packages
include the file because several licenses (MIT, BSD, Apache-2.0) require their text to accompany a binary.
"""
import json
import os
import subprocess
import sys

PATTERNS = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE")


def main() -> int:
    out = sys.argv[1] if len(sys.argv) > 1 else "dist/THIRD_PARTY_LICENSES.txt"
    cmd = ["cargo", "metadata", "--format-version", "1", "--locked"]
    if len(sys.argv) > 2:
        cmd += ["--filter-platform", sys.argv[2]]
    meta = json.loads(subprocess.check_output(cmd, text=True))
    members = set(meta["workspace_members"])
    # the packages reachable from the workspace (normal and build dependencies; not dev dependencies)
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    seen, stack = set(), [m for m in members]
    while stack:
        i = stack.pop()
        if i in seen:
            continue
        seen.add(i)
        for d in nodes[i]["deps"]:
            if any(k.get("kind") in (None, "build") for k in d["dep_kinds"]):
                stack.append(d["pkg"])
    pkgs = {p["id"]: p for p in meta["packages"]}
    libs = sorted((pkgs[i] for i in seen if i not in members and pkgs[i].get("source")), key=lambda p: (p["name"], p["version"]))
    lines = [
        "Third-party libraries in RadioFoundry CPS",
        "=" * 41,
        "",
        "RadioFoundry CPS is MIT licensed (see LICENSE). It is built from the libraries below, each under its own license.",
        "The list is for the platform this package is for.",
        "",
        f"{len(libs)} libraries.",
        "",
    ]
    index = []
    body = []
    for p in libs:
        d = os.path.dirname(p["manifest_path"])
        files = sorted(f for f in os.listdir(d) if f.upper().startswith(PATTERNS) and os.path.isfile(os.path.join(d, f)))
        lic = p.get("license") or ("see " + p["license_file"] if p.get("license_file") else "unknown")
        index.append(f"{p['name']} {p['version']}: {lic}")
        body += ["", "-" * 78, f"{p['name']} {p['version']}", f"License: {lic}"]
        if p.get("repository"):
            body.append(f"Source: {p['repository']}")
        if p.get("authors"):
            body.append("Authors: " + "; ".join(a.split("<")[0].strip() for a in p["authors"]))
        if not files:
            body += ["(no license file ships with this crate; its license is the expression above)"]
        for f in files:
            body += ["", f"--- {f} ---", open(os.path.join(d, f), encoding="utf-8", errors="replace").read().rstrip()]
    os.makedirs(os.path.dirname(out) or ".", exist_ok=True)
    with open(out, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines + index + body) + "\n")
    print(f"{out}: {len(libs)} libraries")
    return 0


if __name__ == "__main__":
    sys.exit(main())
