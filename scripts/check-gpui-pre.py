#!/usr/bin/env python3
"""Compares the gpui-pre crates cargo downloaded with Zed's source at the commit they claim.

gpui-pre is a crates.io republish of Zed's gpui (Zed's own crates.io release is stale, and its git
repo can't be a cargo dependency: a lint fixture there is also named `gpui`). Run this after
bumping the version, against a Zed checkout at the commit in the crates' descriptions:

    git clone https://github.com/zed-industries/zed /tmp/zed && git -C /tmp/zed checkout <commit>
    scripts/check-gpui-pre.py 0.3.9 /tmp/zed

Expected differences are the republisher's labeled packaging edits ("Modified for gpui-pre").
"""
import filecmp, os, re, sys

version, zed = sys.argv[1], sys.argv[2]
registry = next(
    os.path.join(root, d)
    for root in [os.path.expanduser("~/.cargo/registry/src")]
    for d in os.listdir(root)
)
packages = {}
for root, dirs, files in os.walk(zed):
    dirs[:] = [d for d in dirs if d not in ("target", ".git", "node_modules", "test_fixture")]
    if "Cargo.toml" in files:
        m = re.search(r'^\[package\][^\[]*?^name\s*=\s*"([^"]+)"', open(os.path.join(root, "Cargo.toml")).read(), re.M | re.S)
        if m:
            packages[m.group(1)] = root

skip = {"Cargo.toml", "Cargo.toml.orig", "Cargo.lock", ".cargo_vcs_info.json", ".cargo-ok"}
for crate in sorted(d for d in os.listdir(registry) if d.startswith("gpui-pre") and d.endswith("-" + version)):
    name = crate[: -len(version) - 1]
    short = "gpui" if name == "gpui-pre" else name[len("gpui-pre-"):].replace("-", "_")
    src = packages.get("gpui_" + short) or packages.get(short)
    if not src:
        print(f"{crate}: no matching Zed crate")
        continue
    changed = []
    for root, _, files in os.walk(os.path.join(registry, crate)):
        for f in files:
            rel = os.path.relpath(os.path.join(root, f), os.path.join(registry, crate))
            theirs = os.path.join(src, rel)
            if rel not in skip and not (os.path.exists(theirs) and filecmp.cmp(os.path.join(root, f), theirs, shallow=False)):
                changed.append(rel)
    print(f"{crate:34} {os.path.relpath(src, zed):38} {'identical' if not changed else 'differs: ' + ', '.join(changed)}")
