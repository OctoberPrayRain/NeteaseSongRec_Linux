#!/usr/bin/env python3
"""Create client and corresponding source archives with checksums."""

import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
APP = "netease-songrec-linux"


def copy(source, destination, executable=False):
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    destination.chmod(0o755 if executable else 0o644)


def archive(directory, output):
    with tarfile.open(output, "w:gz") as tar:
        tar.add(directory, arcname=directory.name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=ROOT / "target/release" / APP)
    parser.add_argument("--output", type=Path, default=ROOT / "dist")
    args = parser.parse_args()
    version = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
    if os.environ.get("GITHUB_REF_TYPE") == "tag":
        if os.environ["GITHUB_REF_NAME"] != f"v{version}":
            raise SystemExit("Release tag must match Cargo.toml version")
    if not args.binary.is_file():
        raise SystemExit(f"Build the binary first: {args.binary}")
    args.output = args.output.resolve()
    args.output.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="netease-release-", dir=args.output) as temporary:
        staging = Path(temporary)
        name = f"{APP}-{version}-linux-x86_64"
        client = staging / name
        data = client / "share" / APP
        copy(args.binary, client / "bin" / APP, executable=True)
        copy(ROOT / "assets/netease/fingerprint_bridge.js", data / "netease/fingerprint_bridge.js")
        copy(ROOT / "scripts/sync-netease-resources.py", data / "scripts/sync-netease-resources.py")
        copy(ROOT / "packaging/netease-songrec-linux.desktop", client / "share/applications/netease-songrec-linux.desktop")
        copy(ROOT / "assets/ui/recognition-fallback.svg", client / "share/icons/hicolor/scalable/apps/netease-songrec-linux.svg")
        for filename in ["LICENSE", "THIRD_PARTY_NOTICES.md", "LICENSES/GPL-3.0.txt"]:
            copy(ROOT / filename, client / "share/licenses" / APP / filename)
        copy(ROOT / "README.md", client / "share/doc" / APP / "README.md")
        binary_archive = args.output / f"{name}.tar.gz"
        archive(client, binary_archive)

        sources = staging / f"{APP}-{version}-source"
        sources.mkdir()
        source_tar = staging / "source.tar"
        with source_tar.open("wb") as output:
            subprocess.run(["git", "archive", "HEAD"], cwd=ROOT, stdout=output, check=True)
        with tarfile.open(source_tar) as tar:
            tar.extractall(sources, filter="data")
        subprocess.run(["cargo", "vendor", "--locked", str(sources / "vendor/cargo")], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
        config = sources / ".cargo/config.toml"
        config.parent.mkdir(exist_ok=True)
        config.write_text('[source.crates-io]\nreplace-with = "vendored-sources"\n\n[source.vendored-sources]\ndirectory = "vendor/cargo"\n')
        source_archive = args.output / f"{APP}-{version}-source.tar.gz"
        archive(sources, source_archive)

    outputs = [binary_archive, source_archive]
    checksums = "".join(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}\n" for path in outputs)
    (args.output / "SHA256SUMS").write_text(checksums)
    for path in outputs:
        print(path)


if __name__ == "__main__":
    main()
