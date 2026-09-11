#!/usr/bin/env python3
"""Keep the current platform's canonical release files after a successful build."""

import argparse
from pathlib import Path
import re


def prune(output: Path, platform: str, release: str) -> None:
    prefix = f"NoiseHoiHoi-{release}"
    suffixes = {
        "windows": ["-setup.exe", ".exe", "-audio-smoke.exe"],
        "linux": ["-x86_64.AppImage", "-x86_64.AppImage.sha256"],
    }[platform]
    keep = {output / (prefix + suffix) for suffix in suffixes}
    if any(not path.is_file() or path.stat().st_size == 0 for path in keep):
        raise SystemExit("Current release is incomplete; old builds were not removed")

    version = r"NoiseHoiHoi-v\d+\.\d+(?:\.\d+)?"
    patterns = {
        "windows": re.compile(
            version + r"(?:-r\d+)?(?:-setup|-audio-smoke)?\.exe(?:\.sha256)?"
            + "|" + version + r"-diagnostic-windows(?:-r\d+)?\.zip"
        ),
        "linux": re.compile(version + r"(?:-r\d+)?-x86_64\.AppImage(?:\.sha256)?"),
    }
    removed = []
    # Include archived release copies; retain current tools and validation data.
    for path in sorted(output.rglob("NoiseHoiHoi-v*")):
        if path.is_file() and path not in keep and patterns[platform].fullmatch(path.name):
            removed.append((path, path.stat().st_size))
            path.unlink()

    checksums = re.compile(version + r"(?:-r\d+)?-SHA256SUMS\.txt")
    for path in sorted(output.rglob("NoiseHoiHoi-v*")):
        if path.is_file() and checksums.fullmatch(path.name):
            # Both platform builds may finish at the same time.
            try:
                size = path.stat().st_size
                lines = path.read_text().splitlines()
            except FileNotFoundError:
                continue
            names = [line.split(maxsplit=1)[1].lstrip("*")
                     for line in lines if len(line.split(maxsplit=1)) == 2]
            if names and not any((path.parent / Path(name).name).exists() for name in names):
                removed.append((path, size))
                path.unlink(missing_ok=True)
    size = sum(size for _, size in removed)
    print(f"Removed {len(removed)} obsolete {platform} build files ({size / 1024**2:.1f} MiB)")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("platform", choices=["linux", "windows"])
    parser.add_argument("release", help="Canonical release label, e.g. v0.8")
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    if not re.fullmatch(r"v\d+\.\d+", args.release):
        parser.error("release must use vMAJOR.MINOR format")
    prune(args.output.resolve(), args.platform, args.release)
