#!/usr/bin/env python3
"""Package-only Ed25519 signing. The durable key is a GitHub Actions secret."""
import base64
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess
import tempfile
import time
import tomllib

ROOT = Path(__file__).resolve().parent.parent
TAG = os.environ["RELEASE_TAG"]
RENEW = os.environ.get("RENEW_EXISTING") == "1"
if not re.fullmatch(r"V[0-9]+\.[0-9]+\.[0-9]+", TAG):
    raise SystemExit("An exact stable Vmajor.minor.patch tag is required")
VERSION = TAG[1:] if RENEW else tomllib.loads((ROOT / "Cargo.toml").read_text())["package"]["version"]
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
public = base64.b64decode((ROOT / "assets/update-public-key.base64").read_text().strip(), validate=True)
if len(public) != 32:
    raise SystemExit("Pinned Ed25519 public key has not been provisioned; release blocked")
metadata = PACKAGES / "update-metadata.json"
if RENEW:
    # Expiry is deliberately allowed here: renewal authenticates the original
    # exact package set before extending validity. Never bless replaced assets.
    old_bytes = metadata.read_bytes()
    if len(old_bytes) > 65536:
        raise SystemExit("Previous metadata is oversized")
    signature_file = PACKAGES / "update-metadata.sig"
    if signature_file.stat().st_size > 256:
        raise SystemExit("Previous signature exceeds size limit")
    old_signature = base64.b64decode(signature_file.read_text().strip(), validate=True)
    if len(old_signature) != 64:
        raise SystemExit("Invalid previous signature")
    with tempfile.TemporaryDirectory(prefix="editortool-verify-") as tmp:
        folder = Path(tmp)
        (folder / "public.der").write_bytes(bytes.fromhex("302a300506032b6570032100") + public)
        (folder / "signature").write_bytes(old_signature)
        subprocess.run(["openssl", "pkeyutl", "-verify", "-pubin", "-keyform", "DER", "-inkey", str(folder / "public.der"), "-rawin", "-in", str(metadata), "-sigfile", str(folder / "signature")], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    old = json.loads(old_bytes)
    if (set(old) != {"schema", "issued_at", "expires_at", "version", "tag", "assets"}
        or old["schema"] != 1 or old["version"] != VERSION or old["tag"] != TAG
        or old["expires_at"] <= old["issued_at"]
        or sorted(old["assets"], key=lambda a: a["name"]) != assets):
        raise SystemExit("Renewal refused: original signed inventory does not match the downloaded packages")
if os.environ.get("VERIFY_ONLY") == "1":
    print("Existing signature and all 16 package hashes verified; no signing key used.")
    raise SystemExit(0)
issued = int(time.time())
metadata.write_bytes(json.dumps({"schema": 1, "issued_at": issued, "expires_at": issued + 180 * 86400, "version": VERSION, "tag": TAG, "assets": assets}, sort_keys=True, separators=(",", ":")).encode())
(PACKAGES / "SHA256SUMS.txt").write_text("".join(f"{a['sha256']}  {a['name']}\n" for a in assets))
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
