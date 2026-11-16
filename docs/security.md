# Security design and recovery — V1.5.6

## Scope and limitations

EditorTool uses authenticated private-network sharing over TLS/TCP, not a cloud account or public Internet service. This release hardens stored data, consent, revision provenance, quotas and the update supply chain. It does not provide protection against an attacker controlling the unlocked application, user account or operating system. It does not scan documents for malware or execute received code. Backups, snapshots, swap and previously deleted plaintext may retain sensitive data. Do not describe these controls as a security certification or fully post-quantum authentication.

## Local content vault

A random 256-bit profile master key protects files beneath `session`, `history` and `sharing`, except the session process lock and local IPC rendezvous file. Settings, size-limit configuration, diagnostic logs and communication logs are outside this content vault; they can contain local metadata and should be reviewed before export. Files manually saved by the user retain their normal format.

Windows uses user-bound DPAPI. macOS uses Keychain and Linux uses Secret Service, with native KDE Wallet 6/5 as a fallback when Secret Service storage cannot be used. Flatpak has explicit access to the Secret Service name, without a broad home-directory grant. These credential services have their own account/unlock policies and may prompt. Portable profiles use a password. If OS credential storage is unavailable on first setup, the user can explicitly choose password protection or leave protected data locked. There is no silent plaintext fallback.

Installed password profiles can explicitly switch to the system key store after unlocking, using the same master key. Conversion persists a password-wrapped fallback header, verifies the OS credential and the committed header, and attempts to restore the original header on confirmation failure. The fallback remains sensitive offline password-guessing material and must be protected with the profile; it is not a plaintext master key. If OS retrieval later fails, a matching profile/canary permits unlocking that session with the previous password. Keep a separately exported recovery key and encrypted-profile backup. Portable profiles retain password protection; OS key-store prompts remain under operating-system policy.

Password wrapping uses PBKDF2-HMAC-SHA256 with 600,000 iterations, a random 16-byte salt and AES-256-GCM; passwords must contain at least 12 characters. A strong unique password remains necessary. `security/vault.json` contains the public profile identifier, wrapping metadata and an authenticated canary, never the unwrapped master key. The migration-complete state is bound into that canary.

Content uses a random 32-byte file salt, HKDF-SHA256 with the relative path as context, and independent 64-KiB AES-256-GCM records with sequential nonces. Record length/index and an authenticated final record reject modification, truncation, reordering and ordinary file substitution. File encryption/decryption streams through bounded buffers. This is a versioned application format, not a general-purpose audited cryptographic file standard. An attacker able to roll back the entire local profile can also roll back its authenticated state; there is no external monotonic counter.

## Migration and recovery

The first successful unlock persists and verifies the wrapped master key before converting legacy content using atomic replacements. The existing certificate and private key migrate into one authenticated identity bundle, with a consistency check before legacy files are removed. Interrupted migration can resume. After completion, plaintext replacement of protected files is refused rather than re-imported every launch. Damaged or unavailable keys do not cause automatic identity regeneration. A locked editor can still edit/manual-save ordinary files; protected session recovery/history and sharing remain unavailable.

Export an encrypted recovery key from Settings → Security and keep it outside the profile with a strong password stored separately. This export wraps the master key, not the documents themselves; keep a separate backup of the encrypted profile too. Copying an OS-bound profile to another machine requires recovery. Startup offers recovery import when the key/header cannot be unlocked; it checks authentication and an existing encrypted file before committing recovered key metadata where available. Restart after recovery or credential-service changes. Without the master key or valid recovery export, encrypted data is unrecoverable. Never upload profile contents or recovery exports to issue reports.

## Transport, identity and consent

TLS 1.3 requires AES-256-GCM and hybrid X25519 + ML-KEM-768 key exchange. Device identity and revision signatures use classical ECDSA P-256. New TLS connections negotiate fresh traffic secrets; the persistent device key identifies the installation and does not rotate on each connection. Transport remains TCP. Private routed networks and VPNs are supported; no UDP/QUIC fallback exists.

Globally paired clients may request new document offers, which recipients must separately accept. Group membership grants only that group's document/proof access and does not pair every group participant globally. Revoked fingerprints are durably denied, including group members. Revocation invalidates active network generations; permission is rechecked before transfer commits and between outgoing body chunks. Data already sent cannot be recalled. Explicit unblocking permits new pairing rather than granting document access.

Settings can schedule identity replacement for restart, after stopping local sharing. An authenticated request bound to the current certificate avoids repeated replacement after an interrupted restart. Revoke the old fingerprint on every peer and approve the new fingerprint there. Replacing one local key cannot remotely revoke a stolen copy.

## Signed revisions

Protocol 155 requires compatible V1.5.5 peers. Valid V1.5.3/V1.5.4 signatures are revalidated and retained; unverifiable older groups remain paused until explicit repair/new-group consent. Signed tree commits bind document/group, contents, author/name/counters, actual content parents, separate counter provenance and commit type. Resolutions bind the chosen version or explicitly merged contents. Recovered branches name the approved root without fabricating content ancestry. Legacy signatures retain their original payload domain; force priority is not a winner rule.

Recipients verify the pinned author certificate, approved root, membership, exact counter derivation and duplicate author counters before promotion. Missing/unverifiable proofs remain staged or quarantined. Valid ancestry determines fast-forward; concurrent contents/decisions require user review. Neither timestamps nor counters alone establish a winner.

Proofs are group-scoped, at most 32 nodes per message and 512 unseen nodes per resumable round. Retained graph nodes keep available bodies, subject to quotas and explicit retired-group cleanup. Root recovery is limited to already paired approved document members and requires confirmation; new groups always require recipient consent. See [Sync history and recovery](sync-history.md) for backups, missing bases and limits. Signatures establish who signed an edit, not whether the author or document is safe.

## Automatic saving and quotas

Automatic writes to recognised code extensions, Unix executable files and known startup/security-sensitive paths require explicit per-document consent bound to the canonical target path. Save As, path retargeting or a changed target invalidates approval. This is a bounded heuristic, not exhaustive detection of executable formats. Manual saving remains an explicit user action.

Default retained sync content limits are 1 GiB total, 512 MiB per author and 256 MiB per document, with 256 groups and 8,192 signed proof nodes. Settings → Security can adjust them within bounded ranges. Sharing metadata has an independent 32-MiB ceiling. Concurrent uploads/local writes reserve space; total accounting includes stored encrypted bodies and pending reservations and is intentionally conservative. Shared content can count against more than one author/document attribution. Reducing a limit does not automatically delete existing data. A quota failure refuses further writes, and a metadata persistence failure stops sharing.

Explicit cleanup removes retired sync groups and their no-longer-needed proof records, then unreferenced retained bodies. Active graph history, staged/quarantined records, pending drafts and pending offers/inbox/outbox remain referenced while their groups are retained. Open tabs/session cache and original saved files are separate and preserved. Cleanup is refused during reserved content transfers. It does not securely erase storage media and discards retired sync-only history, so review the confirmation first.

## Logs and dependency advisories

Traffic logs contain protocol event labels, byte counts and endpoint/device metadata, not document bodies or private keys. Pseudonymised export replaces device identity/name/address fields with stable labels for that export, while retaining timing and traffic sizes. Such patterns can still identify activity; review before sharing. Diagnostic logs also need review.

`scripts/dependency-advisories.py` submits public locked crate names/versions to OSV and runs on pull requests, manual dispatch and a weekly schedule. Vulnerability entries fail the check; withdrawn advisories are ignored and informational maintenance notices are reported separately. The V1.5.3 lookup covered 277 registry dependencies: no vulnerability entries and seven informational notices concerning `ttf-parser`, `zbus_names`, `zvariant` and `zvariant_derive`. Several affected standalone zbus crates moved into the zbus 6 workspace; their old major branches have no replacement patch. These transitive dependency migrations remain maintenance work, not silently resolved findings. A database lookup cannot establish absence of vulnerabilities.

## Update trust

See [update signing](update-signing.md). The pinned Ed25519 trust key is unchanged. Signing is restricted to a main-only environment; metadata renewal authenticates both previous metadata and original package hashes before extending validity. Release assets alone cannot be replaced with valid new signatures. Compromised trusted workflow code, signing administrators or an unlocked signing-key recovery account remain inside the trust boundary. Platform binaries are not Windows developer-signed or Apple-notarised.
