# Signing update metadata

The Ed25519 update key is independent of Windows and Apple developer certificates. Its private key is stored permanently in the repository Actions secret `UPDATE_SIGNING_KEY_B64`, as Base64-encoded PKCS#8 PEM. Only its raw 32-byte public key, encoded as Base64, belongs in `assets/update-public-key.base64`. An empty public-key file explicitly disables update installation and release signing.

Before creating the first key, confirm that authenticated access can write and verify the existence of the permanent Actions secret. Never put private keys into Git, chat, command-line arguments, artifacts or logs. Temporary copies require a private directory and permissions limited to their owner. Remove a temporary private copy only after confirming durable storage. Actions secrets cannot subsequently be read back. Key replacement requires the previous signing capability or a manually trusted migration.

The release job signs the exact bytes of `update-metadata.json`. Before signing, it checks the tag, version, all 16 package names and that the secret's public key matches the pinned public key. Metadata binds the version, package names, lengths, SHA-256 hashes and a 180-day expiry. The app remembers the highest authenticated version it has seen. Incorrect system time can prevent updates.

`update-metadata.sig` contains a Base64-encoded Ed25519 signature. The app requires a valid signature and a strictly newer version before downloading, checks again before handing over to the installer, and checks the package hash again in the installation helper after the old process exits. There is no fallback to unsigned checksums. Clients through V1.4.9 obtain this trust anchor only through a manually trusted upgrade; their old verification behaviour cannot be hardened retroactively.

Signing is a packaging operation. The current workflow runs no automated, interactive or installation tests and does not launch the app. All six native builds and every installer must complete before publication.

The repository owner chose an Actions secret for key storage. Anyone able to change the signing workflow or administer the secret is consequently inside the trust boundary: a compromised workflow can use the signing key. The signature protects against replacement of release files alone; it does not protect against compromise of the release/signing environment. Key rotation needs an already trusted application version.

Initial provisioning is complete. A recovery copy is stored outside the repository, encrypted with Windows DPAPI and tied to the local Windows user. No private copy belongs in Git or release artifacts. This update-signing key is separate from each installation's persistent device-identity key used for document sharing.

Before metadata expires, run **Renew signed update metadata** in Actions with the existing release tag. It downloads the same 16 packages, confirms the signing key matches, sets a new validity period and replaces only the two metadata files. A check during the brief two-file replacement may safely reject the update; retry after renewal completes.
