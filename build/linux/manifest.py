#!/usr/bin/env python3
"""Merges freshly built archives into the manifest the app reads.

    manifest.py <dir with *.tar.gz and *.sha256> [previous manifest.json] > manifest.json

Builds already in the previous manifest are kept (their files stay on the release), so
publishing PHP 8.4.28 doesn't drop 8.4.27 for people who still use it.
"""
import json
import pathlib
import re
import sys

NAME = re.compile(r"^(apache|nginx|php|redis|memcached|minio)-(.+)-(linux|macos|windows)-(x86_64|aarch64)\.tar\.gz$")


def label(runtime, version):
    # nginx.org: odd minor versions are mainline, even ones stable.
    if runtime == "nginx":
        return "Mainline" if int(version.split(".")[1]) % 2 else "Stable"
    if runtime == "minio":
        return "Unmaintained upstream"
    return None


def main():
    out = pathlib.Path(sys.argv[1])
    entries = {}
    if len(sys.argv) > 2 and pathlib.Path(sys.argv[2]).is_file():
        for e in json.loads(pathlib.Path(sys.argv[2]).read_text())["releases"]:
            entries[(e["runtime"], e["version"], e.get("os", "linux"), e["arch"])] = e
    for archive in sorted(out.glob("*.tar.gz")):
        m = NAME.match(archive.name)
        if not m:
            continue
        runtime, version, os_name, arch = m.groups()
        sha256 = (out / f"{archive.name}.sha256").read_text().split()[0]
        entries[(runtime, version, os_name, arch)] = {
            "runtime": runtime,
            "version": version,
            "os": os_name,
            "arch": arch,
            "file": archive.name,
            "sha256": sha256,
            "label": label(runtime, version),
        }
    releases = sorted(entries.values(), key=lambda e: (e["runtime"], e["os"], e["arch"], [int(p) for p in re.findall(r"\d+", e["version"])]))
    json.dump({"formatVersion": 1, "releases": releases}, sys.stdout, indent=2)
    sys.stdout.write("\n")


main()
