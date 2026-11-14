# EditorTool V1.5.3

## Local data and identity protection

Session contents, local file history, sharing metadata, retained sync contents and the device private key now use authenticated AES-256-GCM encryption. Installed profiles protect their master key with Windows DPAPI, macOS Keychain or Linux Secret Service. Portable profiles use a password; installations without an available credential service offer the same explicit password option. There is no silent plaintext fallback.

The first successful unlock automatically migrates existing cached data and preserves the device identity. If unlocking or migration fails, protected data stays locked and sharing remains unavailable. The editor can still edit and manually save ordinary documents. It never silently creates a replacement identity for locked or damaged existing key material. Original files saved by the user retain their normal UTF-8 or `.etdoc` format.

Settings → Security provides password-encrypted recovery-key export and startup offers recovery-key import when protection cannot be unlocked. Keep the export and its password safe, outside the profile. Without the master key or a valid recovery export, encrypted data cannot be recovered. Migration cannot securely erase old filesystem blocks, backups or snapshots.

## Device permissions and signed document history

Global pairing approval and document membership are separate permissions. Accepting a sync group does not globally trust every participant. Revoked certificate fingerprints are retained, checked before commits and between outgoing body chunks, and cannot be silently re-added. Explicit unblocking permits a fresh pairing request; it does not grant access by itself.

Each new document revision signs its content hash, size, group membership, author, document identifier, parent revisions and causal counters using the existing device identity. Peers verify provenance and parent counters rather than accepting arbitrary version clocks. Conflicting edits still require a user choice; detected forks from the same device counter are rejected.

**Upgrade all communicating devices to V1.5.3 and share existing documents as fresh groups.** Earlier protocols are refused. Old groups are archived locally; document contents and cached editor tabs are preserved. Do not downgrade an encrypted profile in place; older versions cannot read this storage format. A peer more than 512 unseen signed revisions behind may need a fresh group.

Device authentication and revision signatures use classical ECDSA P-256. Transport retains TLS 1.3, AES-256-GCM and hybrid X25519 + ML-KEM-768 key exchange over TCP. No UDP or QUIC transport is introduced. Settings can explicitly schedule identity replacement for the next restart; revoke the old fingerprint on every other device and approve the new identity there. Replacement cannot remotely invalidate a stolen key.

## Safer saving, quotas and diagnostics

- Automatic sync writes to recognised code files, executable files and startup-sensitive locations require explicit approval bound to that document and exact local target. Changing the target invalidates that approval. Received data is never executed by EditorTool. Path/extension checks are a safeguard, not a malware scanner.
- Adjustable retained-sync-content limits: 1 GiB total, 512 MiB per author and 256 MiB per document by default; 256 groups and 8,192 signed proof nodes. Sharing metadata is capped at 32 MiB. Reservations include concurrent transfers. Exceeding a limit rejects further work instead of silently deleting history.
- Explicit cleanup removes retired sync groups and unused retained bodies while preserving current editor tabs, saved files and active group heads/conflicts/recent history. Cleanup is refused while content is being committed. It is not secure media erasure.
- Communication logs offer pseudonymised export with stable device labels and no device names, addresses or fingerprints. Timestamps and traffic sizes remain visible; review exports before sharing.
- Weekly and pull-request dependency advisory checks report vulnerabilities separately from maintenance notices. The release lookup reported no vulnerability entries for the locked dependencies and seven informational notices concerning unmaintained transitive crates; this is not a security certification.

## Update supply chain

The existing Ed25519 update public key is unchanged. Its private key is now scoped to the main-only `release-signing` GitHub environment. Release and renewal jobs use protected main source; renewal verifies the previous signature and every original package size/hash before extending metadata validity. Repository-wide access to the signing secret has been removed. CODEOWNERS, required pull requests/build checks and dependency update configuration are included. Repository administrators retain bypass permissions and remain inside the signing trust boundary.

## Packages and validation

All 16 platform packages: Windows setup EXE/portable ZIP; Linux Flatpak/RPM/DEB/portable TAR.GZ; macOS DMG/app ZIP; each for x64 and ARM64. Also included: `SHA256SUMS.txt`, `update-metadata.json` and `update-metadata.sig`.

Source review, dependency advisory lookup, native release builds and packaging only. No automated, interactive or installation tests and no app launches, as requested. Windows packages have no developer certificate; macOS bundles are ad-hoc signed and not notarised. Original code remains under The Unlicense; dependency licences remain included.

See [the security design](https://github.com/OnlySecci/EditorTool/blob/main/docs/security.md) for recovery, migration and threat-model limitations. These controls do not protect an unlocked application against an attacker controlling the same user account or operating system.
