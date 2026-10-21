#!/usr/bin/env python3
"""Keep original NetEase resources separate and sync the required runtime files."""

import argparse
import hashlib
import io
import json
from pathlib import Path
import shutil
import sys
import urllib.request
import zipfile

PROJECT_ROOT = Path(__file__).resolve().parent.parent
VERSION = "1.0.4"
DOWNLOAD_URL = (
    "https://clients2.google.com/service/update2/crx?response=redirect"
    "&prodversion=130.0.0.0&acceptformat=crx3"
    "&x=id%3Dkemcalcncfhmdkgglekijclbomdoohkp%26installsource%3Dondemand%26uc"
)
# These exact files are used by the existing bridge. An upstream update needs
# an explicit compatibility check before changing the version and checksums.
FILES = {
    "afp.wasm": ("netease/afp.wasm", "8064415bb66e45410e88877f07cd007a3ae62ebf463aac13c159a8f13e4c8fe8"),
    "sandbox.bundle.js": ("netease/sandbox.bundle.js", "ac38899d27fcc81cbae644bd89ec826ebb865e93c759521ab5c7c8875ed83c1e"),
    "icon-128.png": ("ui/recognition-128.png", "fdf756cfb23855a9633317636d1351b6832c4cc4227b9ced4911249fb70bcccf"),
}


def verify(files):
    version = json.loads(files["manifest.json"])["version"]
    if version != VERSION:
        raise ValueError(f"Expected extension {VERSION}, received {version}; review bridge compatibility before updating")
    for name, (_, expected) in FILES.items():
        actual = hashlib.sha256(files[name]).hexdigest()
        if actual != expected:
            raise ValueError(f"Unexpected contents of {name}; the official extension may have changed")


def read_directory(directory):
    return {name: (directory / name).read_bytes() for name in ["manifest.json", *FILES]}


def read_archive(data):
    # zipfile also handles Chrome CRX archives with a signature before the ZIP.
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        manifests = [name for name in archive.namelist() if name.rsplit("/", 1)[-1] == "manifest.json"]
        if len(manifests) != 1:
            raise ValueError("Expected a single extension manifest in the archive")
        prefix = manifests[0][:-len("manifest.json")]
        return {name: archive.read(prefix + name) for name in ["manifest.json", *FILES]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--from-dir", type=Path, help="import an already unpacked official extension")
    source.add_argument("--archive", type=Path, help="import an official ZIP or CRX without downloading")
    source.add_argument("--refresh", action="store_true", help="download again instead of using the local original files")
    parser.add_argument("--cache-dir", type=Path, default=PROJECT_ROOT / "vendor/netease-extension")
    parser.add_argument("--destination", type=Path, default=PROJECT_ROOT / "assets", help="application data directory to sync into")
    args = parser.parse_args()

    if args.from_dir:
        files = read_directory(args.from_dir)
    elif args.archive:
        files = read_archive(args.archive.read_bytes())
    elif not args.refresh and (args.cache_dir / "manifest.json").is_file():
        files = read_directory(args.cache_dir)
    else:
        print(f"Downloading NetEase extension {VERSION} from the official Chrome Web Store…", flush=True)
        request = urllib.request.Request(DOWNLOAD_URL, headers={"User-Agent": "NeteaseSongRec-Linux resource installer"})
        with urllib.request.urlopen(request, timeout=60) as response:
            files = read_archive(response.read())

    verify(files)
    args.cache_dir.mkdir(parents=True, exist_ok=True)
    for name, content in files.items():
        (args.cache_dir / name).write_bytes(content)
    print(f"Official originals: {args.cache_dir.resolve()}")
    for name, (relative, _) in FILES.items():
        destination = args.destination / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(args.cache_dir / name, destination)
        print(f"  {name} -> {destination.resolve()}")
    print(f"NetEase {VERSION} resources are ready. Original files and synced copies must remain outside Git/release archives.")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, zipfile.BadZipFile) as error:
        print(f"Could not sync NetEase resources: {error}", file=sys.stderr)
        sys.exit(1)
