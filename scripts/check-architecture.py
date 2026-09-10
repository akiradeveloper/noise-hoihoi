#!/usr/bin/env python3
"""Keep native/UI dependencies out of the portable libraries (normal dependencies)."""
import json
from pathlib import Path
import subprocess

root = Path(__file__).resolve().parent.parent
metadata = json.loads(subprocess.check_output(
    ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=root
))
# Changes to these boundaries must be an explicit architectural decision.
allowed = {
    "noise-hoihoi-engine": {"thiserror", "rtrb", "rubato"},
    "noise-hoihoi-session": {"noise-hoihoi-engine", "serde"},
    "noise-net": {"anyhow", "burn", "burn-store", "deep_filter", "hound", "thiserror"},
}
checked = set()
for package in metadata["packages"]:
    name = package["name"]
    if name not in allowed:
        continue
    checked.add(name)
    for dependency in package["dependencies"]:
        if dependency["kind"] == "dev":
            continue  # Hardware integration tests may use the native runtime.
        if (dependency["kind"] is not None or dependency["target"] is not None
                or dependency["name"] not in allowed[name]):
            raise SystemExit(f"Architecture violation: {name} -> {dependency['name']} "
                             f"({dependency['kind']}, {dependency['target']})")
    if any(target["kind"] == ["custom-build"] for target in package["targets"]):
        raise SystemExit(f"Architecture violation: build script in {name}")
if checked != allowed.keys():
    raise SystemExit(f"Missing portable crates: {allowed.keys() - checked}")
print("Architecture boundaries passed: engine, session, NoiseNet")
