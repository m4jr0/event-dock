#!/usr/bin/env python3
"""Check permitted internal Cargo dependencies, including dev and build dependencies."""
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[1]
ALLOWED = {
    "eventdock-core": set(),
    "eventdock": {"eventdock-core"},
    "eventdock-engine": {"eventdock-core"},
    "eventdock-cli": {"eventdock", "eventdock-engine", "eventdock-core"},
    "eventdock-gui": {"eventdock", "eventdock-engine", "eventdock-core"},
}


def check_boundaries(metadata):
    packages = [p for p in metadata["packages"] if p["id"] in metadata["workspace_members"]]
    names = {p["name"] for p in packages}
    errors = []
    if names != set(ALLOWED):
        errors.append(f"Workspace membership differs from policy: {sorted(names ^ set(ALLOWED))}")
    for package in packages:
        name = package["name"]
        for dependency in package["dependencies"]:
            target = dependency["name"]
            if target in names and target not in ALLOWED.get(name, set()):
                errors.append(f"Forbidden dependency: {name} -> {target} ({dependency.get('kind') or 'normal'})")
    return errors


def main():
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=ROOT, text=True
    ))
    errors = check_boundaries(metadata)
    if errors:
        raise SystemExit("\n".join(errors))
    print("EventDock crate boundaries are valid.")


if __name__ == "__main__":
    main()
