#!/usr/bin/env python3
"""Write cargo-sources.json, the list of crates the Flatpak build vendors.

    python3 build-aux/cargo-sources.py

Run it after every change to Cargo.lock. The output has the format of
flatpak-builder-tools' flatpak-cargo-generator, for crates from crates.io:
each crate is unpacked into cargo/vendor/ with a checksum file, and a Cargo
configuration makes that directory replace the registry. The module that uses
it sets CARGO_HOME to its `cargo` directory.
"""

import json
import pathlib
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"
VENDOR = "cargo/vendor"
CONFIG = """\
[source.vendored-sources]
directory = "cargo/vendor"

[source.crates-io]
replace-with = "vendored-sources"
"""


def main() -> int:
    lock = tomllib.loads((ROOT / "Cargo.lock").read_text())
    sources = []
    for package in lock["package"]:
        source = package.get("source")
        if source is None:
            continue  # a crate of this workspace
        if source != REGISTRY:
            print(f"{package['name']}: unsupported source {source}", file=sys.stderr)
            return 1
        name, version, checksum = package["name"], package["version"], package["checksum"]
        dest = f"{VENDOR}/{name}-{version}"
        sources.append({
            "type": "archive",
            "archive-type": "tar-gzip",
            "url": f"https://static.crates.io/crates/{name}/{name}-{version}.crate",
            "sha256": checksum,
            "dest": dest,
        })
        sources.append({
            "type": "inline",
            "contents": json.dumps({"package": checksum, "files": {}}),
            "dest": dest,
            "dest-filename": ".cargo-checksum.json",
        })
    sources.append({
        "type": "inline",
        "contents": CONFIG,
        "dest": "cargo",
        "dest-filename": "config.toml",
    })
    (ROOT / "cargo-sources.json").write_text(json.dumps(sources, indent=4) + "\n")
    print(f"{(len(sources) - 1) // 2} crates")
    return 0


if __name__ == "__main__":
    sys.exit(main())
