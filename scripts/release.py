#!/usr/bin/env python3
"""Validate and publish this repository's explicit, dependency-ordered release."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]


def run(*args, capture=False):
    result = subprocess.run(args, cwd=ROOT, check=True, text=True, encoding="utf-8",
                            stdout=subprocess.PIPE if capture else None)
    return result.stdout.strip() if capture else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--publish", action="store_true")
    parser.add_argument("--tag")
    options = parser.parse_args()
    plan = json.loads((ROOT / "release.json").read_text(encoding="utf-8"))
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    workspace = manifest.get("workspace", {})
    version = manifest.get("package", {}).get("version") or workspace["package"]["version"]
    expected_tag = plan.get("tag_prefix", "v") + version
    event_tag = os.environ.get("GITHUB_REF_NAME") if os.environ.get("GITHUB_REF_TYPE") == "tag" else None
    tag = options.tag or event_tag or expected_tag
    if tag != expected_tag:
        raise SystemExit(f"Tag {tag!r} must match manifest version: {expected_tag}")
    metadata = json.loads(run("cargo", "metadata", "--no-deps", "--format-version", "1", capture=True))
    target = Path(metadata["target_directory"]).resolve()
    configured = os.environ.get("CARGO_TARGET_DIR")
    if not configured or target != Path(configured).resolve():
        raise SystemExit("Set and verify CARGO_TARGET_DIR before releasing")
    if os.name == "nt" and not str(target).replace("\\", "/").lower().startswith("d:/buildcache/"):
        raise SystemExit("Windows release artifacts must stay under D:/BuildCache")
    packages = {p["name"]: p for p in metadata["packages"]}
    selected = [packages[name] for name in plan["packages"]]
    owned = {p["name"]: p for p in selected}
    completed = set()
    for package in selected:
        if package.get("publish") == []:
            raise SystemExit(f"{package['name']} disables publication")
        if package.get("repository") != "https://github.com/" + plan["repository"]:
            raise SystemExit(f"Wrong repository metadata: {package['name']}")
        if not any(package["name"] == prefix or package["name"].startswith(prefix + "-")
                   or package["name"].startswith(prefix + "_") for prefix in plan["allowed_prefixes"]):
            raise SystemExit(f"Refusing to publish outside owned namespaces: {package['name']}")
        for dependency in package["dependencies"]:
            if dependency["kind"] == "dev":
                continue
            if dependency.get("source", "") and dependency["source"].startswith("git+"):
                raise SystemExit(f"Git dependency would change when published: {package['name']} -> {dependency['name']}")
            if dependency["name"] in owned and dependency["name"] not in completed:
                raise SystemExit(f"Release order must place {dependency['name']} before {package['name']}")
            if dependency.get("path") and dependency["name"] not in owned:
                raise SystemExit(f"Unreleased path dependency: {package['name']} -> {dependency['name']}")
        completed.add(package["name"])
    print(json.dumps({"tag": tag, "repository": plan["repository"],
                      "packages": [{"name": p["name"], "version": p["version"]} for p in selected]}, indent=2), flush=True)
    if not options.publish:
        return
    notes = ROOT / "releases" / (version + ".md")
    if not notes.is_file():
        raise SystemExit("A reviewed release-notes file is required")
    if run("git", "status", "--porcelain", capture=True):
        raise SystemExit("Publication requires a clean checkout")
    head = run("git", "rev-parse", "HEAD", capture=True)
    if run("git", "rev-parse", tag + "^{commit}", capture=True) != head:
        raise SystemExit("Publication must run at the exact release tag")
    if not os.environ.get("CARGO_REGISTRY_TOKEN"):
        raise SystemExit("CARGO_REGISTRY_TOKEN is required")
    cross = set(plan.get("cross_target_packages", []))
    if cross:
        # Cross-target packages cannot be verified by the publisher's native
        # linker. Require the successful validation jobs from this same run.
        if not os.environ.get("GITHUB_ACTIONS") or not os.environ.get("GITHUB_RUN_ID"):
            raise SystemExit("Cross-target publication requires the gated Actions release workflow")
        jobs = json.loads(run("gh", "api", f"repos/{plan['repository']}/actions/runs/{os.environ['GITHUB_RUN_ID']}/jobs?per_page=100", capture=True))
        succeeded = {job["name"] for job in jobs["jobs"] if job["conclusion"] == "success"}
        if not set(plan.get("required_release_jobs", [])).issubset(succeeded):
            raise SystemExit("Required platform validation jobs have not all succeeded")
    artifacts = []
    for package in selected:
        name, package_version = package["name"], package["version"]
        request = urllib.request.Request(f"https://crates.io/api/v1/crates/{name}/{package_version}",
                    headers={"User-Agent": "glory-release/1.0 (https://github.com/" + plan["repository"] + ")"})
        existing = None
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                existing = json.load(response)["version"]
        except urllib.error.HTTPError as error:
            if error.code != 404:
                raise
        if existing:
            if existing.get("repository") != "https://github.com/" + plan["repository"]:
                raise SystemExit(f"Existing {name} {package_version} belongs to another source")
            print(f"Already published: {name} {package_version}", flush=True)
        else:
            command = ["cargo", "publish", "--locked", "-p", name]
            if name in cross:
                command.append("--no-verify")
            run(*command)
        # Let sparse-index propagation finish before building dependent crates.
        ready = False
        for attempt in range(30):
            try:
                with urllib.request.urlopen(request, timeout=30) as response:
                    published = json.load(response)["version"]
                ready = True
                break
            except urllib.error.HTTPError as error:
                if error.code != 404:
                    raise
                time.sleep(2)
        if not ready:
            raise SystemExit(f"Published version did not become visible: {name} {package_version}")
        archive = target / "package" / f"{name}-{package_version}.crate"
        if archive.exists():
            checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
            if checksum != published["checksum"]:
                raise SystemExit(f"Registry checksum differs from packaged artifact: {name}")
            artifacts.append(archive)
    output = target / "release" / tag
    output.mkdir(parents=True, exist_ok=True)
    evidence = output / "release.json"
    evidence.write_text(json.dumps({"repository": plan["repository"], "tag": tag, "commit": head,
        "packages": [{"name": p["name"], "version": p["version"]} for p in selected]}, indent=2) + "\n", encoding="utf-8")
    exists = subprocess.run(["gh", "release", "view", tag, "--repo", plan["repository"]],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0
    if not exists:
        command = ["gh", "release", "create", tag, "--repo", plan["repository"], "--verify-tag",
                   "--title", plan["title"] + " " + version, "--notes-file", str(notes)]
        if "-" in version:
            command.append("--prerelease")
        run(*command, str(evidence), *map(str, artifacts))


if __name__ == "__main__":
    main()
