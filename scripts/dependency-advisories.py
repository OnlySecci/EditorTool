#!/usr/bin/env python3
"""Read-only public dependency advisory lookup; never builds, runs or tests the app."""
import json
from pathlib import Path
import sys
import tomllib
import urllib.request
import urllib.parse

root = Path(__file__).resolve().parent.parent
packages = [p for p in tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"] if p.get("source", "").startswith("registry+")]
request = urllib.request.Request("https://api.osv.dev/v1/querybatch", data=json.dumps({"queries": [{"package": {"name": p["name"], "ecosystem": "crates.io"}, "version": p["version"]} for p in packages]}).encode(), headers={"Content-Type": "application/json"})
with urllib.request.urlopen(request, timeout=60) as response:
    raw = response.read(4 * 1024 * 1024 + 1)
if len(raw) > 4 * 1024 * 1024:
    raise SystemExit("Advisory response exceeded the size limit")
results = json.loads(raw)["results"]
if len(results) != len(packages):
    raise SystemExit("Incomplete advisory response")
found = []
notices = []
details = {}
for package, result in zip(packages, results):
    for advisory in result.get("vulns", []):
        identity = advisory["id"]
        if identity not in details:
            with urllib.request.urlopen("https://api.osv.dev/v1/vulns/" + urllib.parse.quote(identity, safe=""), timeout=30) as response:
                raw = response.read(1024 * 1024 + 1)
            if len(raw) > 1024 * 1024:
                raise SystemExit("Advisory details exceeded the size limit")
            details[identity] = json.loads(raw)
        detail = details[identity]
        affected = [p for p in detail.get("affected", []) if p.get("package", {}).get("name") == package["name"]]
        informational = bool(affected) and all(p.get("database_specific", {}).get("informational") for p in affected)
        if detail.get("withdrawn"):
            continue
        (notices if informational else found).append((package["name"], package["version"], identity))
for name, version, advisory in found:
    print(f"{name} {version}: {advisory} https://osv.dev/vulnerability/{advisory}")
for name, version, advisory in notices:
    print(f"MAINTENANCE NOTICE: {name} {version}: {advisory} https://osv.dev/vulnerability/{advisory}")
print(f"Read-only advisory lookup: {len(packages)} registry dependencies, {len(found)} vulnerability findings, {len(notices)} informational maintenance notices. No application executed.")
sys.exit(bool(found))
