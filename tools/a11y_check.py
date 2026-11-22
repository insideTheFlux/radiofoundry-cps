#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
# Copyright (c) 2026 insideTheFlux
"""Reads what a screen reader is told by a RUNNING RadioFoundry CPS on Linux (AT-SPI): how many controls it publishes, and
which text boxes, drop-downs and check boxes have no name. Read only: it changes nothing in the app.

    cargo run -p rfcps --release -- some-image.bin      # in one terminal; leave the Channels tab showing
    python3 tools/a11y_check.py                          # in another

It needs PyGObject with Atspi (Ubuntu: python3-gi, gir1.2-atspi-2.0), and accessibility switched on for the session: Orca
does that when it runs, or `gsettings set org.gnome.desktop.interface toolkit-accessibility true` (set it back to false
afterwards). Exit status 1 when a control has no name or the app is not visible.
"""
import sys
from collections import Counter

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402

INTERACTIVE = {"text", "entry", "password text", "combo box", "check box", "toggle button", "spin button", "slider", "push button", "button"}


def main() -> int:
    Atspi.init()
    desk = Atspi.get_desktop(0)
    apps = [desk.get_child_at_index(i) for i in range(desk.get_child_count())]
    ours = [a for a in apps if a and (a.get_name() or "").lower() in ("rfcps", "radiofoundry cps")]
    if not ours:
        print("RadioFoundry CPS is not on the accessibility bus. Is it running, and is accessibility switched on?")
        return 1
    roles, unnamed = Counter(), []

    def walk(n, path):
        try:
            role, name = n.get_role_name(), n.get_name() or ""
            roles[(role, bool(name))] += 1
            if role in INTERACTIVE and not name:
                unnamed.append(" > ".join(path[-3:] + [role]))
            for i in range(n.get_child_count()):
                walk(n.get_child_at_index(i), path + [role])
        except Exception:
            pass

    walk(ours[0], [])
    print(f"{sum(roles.values())} nodes published")
    for (role, named), k in sorted(roles.items(), key=lambda x: -x[1])[:12]:
        print(f"  {role:<14} {'named' if named else 'no name':<8} {k}")
    print(f"controls without a name: {len(unnamed)}")
    for p in unnamed[:5]:
        print("  ", p)
    return 1 if unnamed else 0


if __name__ == "__main__":
    sys.exit(main())
