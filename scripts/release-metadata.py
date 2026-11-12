#!/usr/bin/env python3
"""Package-only Ed25519 signing. The durable key is a GitHub Actions secret."""
import base64
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
VERSION = tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
TAG = os.environ["RELEASE_TAG"]
if TAG != f"V{VERSION}":
    raise SystemExit("Release tag must exactly match Cargo.toml")
PACKAGES = ROOT / "packages"
expected = set()
for arch in ("x64", "arm64"):
    expected |= {f"EditorTool-{VERSION}-windows-{arch}-setup.exe", f"EditorTool-{VERSION}-windows-{arch}-portable.zip"}
for arch in ("x86_64", "arm64"):
    expected |= {f"EditorTool-{VERSION}-macos-{arch}.dmg", f"EditorTool-{VERSION}-macos-{arch}.zip"}
for arch, deb in (("x86_64", "amd64"), ("aarch64", "arm64")):
    expected |= {f"EditorTool-{VERSION}-linux-{arch}-portable.tar.gz", f"EditorTool-{VERSION}-linux-{arch}.flatpak", f"editortool-{VERSION}-1.{arch}.rpm", f"editortool_{VERSION}_{deb}.deb"}
actual = {p.name for p in PACKAGES.iterdir() if p.is_file()} - {"SHA256SUMS.txt", "update-metadata.json", "update-metadata.sig"}
if actual != expected:
    raise SystemExit(f"Expected all 16 packages; missing={sorted(expected-actual)}, extra={sorted(actual-expected)}")
assets = []
for name in sorted(expected):
    path = PACKAGES / name
    if path.is_symlink() or not 0 < path.stat().st_size <= 256 * 1024 * 1024:
        raise SystemExit("Invalid package size or type")
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    assets.append({"name": name, "size": path.stat().st_size, "sha256": digest})
metadata = PACKAGES / "update-metadata.json"
metadata.write_bytes(json.dumps({"schema": 1, "version": VERSION, "tag": TAG, "assets": assets}, sort_keys=True, separators=(",", ":")).encode())
(PACKAGES / "SHA256SUMS.txt").write_text("".join(f"{a['sha256']}  {a['name']}\n" for a in assets))
public = base64.b64decode((ROOT / "assets/update-public-key.base64").read_text().strip(), validate=True)
if len(public) != 32:
    raise SystemExit("Pinned Ed25519 public key has not been provisioned; release blocked")
encoded = os.environ.get("UPDATE_SIGNING_KEY_B64", "")
if not encoded:
    raise SystemExit("Durable UPDATE_SIGNING_KEY_B64 Actions secret is required")
with tempfile.TemporaryDirectory(prefix="editortool-signing-", dir=os.environ.get("RUNNER_TEMP")) as temporary:
    directory = Path(temporary)
    key = directory / "private.pem"
    fd = os.open(key, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(base64.b64decode(encoded, validate=True))
    public_der = subprocess.check_output(["openssl", "pkey", "-in", str(key), "-pubout", "-outform", "DER"], stderr=subprocess.DEVNULL)
    # Ed25519 SubjectPublicKeyInfo: RFC 8410 OID 1.3.101.112, 32 raw public bytes.
    if public_der != bytes.fromhex("302a300506032b6570032100") + public:
        raise SystemExit("Signing secret does not match the pinned Ed25519 key")
    signature = directory / "signature"
    subprocess.run(["openssl", "pkeyutl", "-sign", "-rawin", "-inkey", str(key), "-in", str(metadata), "-out", str(signature)], check=True, stderr=subprocess.DEVNULL)
    if signature.stat().st_size != 64:
        raise SystemExit("Invalid signature length")
    (PACKAGES / "update-metadata.sig").write_bytes(base64.b64encode(signature.read_bytes()) + b"\n")
print("All 16 packages hashed; versioned update metadata signed with the pinned Ed25519 key.")
