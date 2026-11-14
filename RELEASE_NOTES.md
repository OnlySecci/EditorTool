# EditorTool V1.5.5

## Shared history instead of force priority

All participating devices are equal. Sequential edits follow their signed content parents and sync after the five-second writing pause. An unchanged offline device catches up automatically. Independent edits remain branches until a user chooses or merges their contents.

The versions view offers **Use selected as main** and **Use editor text as main**. A confirmed choice creates a signed resolution joining the displayed branches. Unseen offline branches and concurrent decisions remain visible for another decision. Clocks and legacy force generations never silently select a winner. Identical contents can be joined without user arbitration.

## Existing links and recovery

- Preserve encrypted pre-migration session/sharing backups, available historical snapshots and independent pending drafts.
- Revalidate V1.5.3/V1.5.4 signatures; recover branches hidden by force priority and store exact signed base IDs in local links.
- Restore missing bases/history in bounded resumable batches. Show document status instead of repeated background error dialogs.
- Add **Repair document sync**: retry restoration, keep text as an independent branch, ask paired devices for a missing root, or start a new approved history. Roots require confirmation; new groups require recipient approval and preserve different local copies.
- Preview/compare historical and linked archived versions or open recovery copies. Unverifiable records cannot masquerade as signed ancestry.
- Archive damaged stored snapshots before authenticated atomic replacement. Preserve pending drafts independently of session writes.

## Status and logs

Show per-document recovery, backlog, integrity, quota and connection states. Log signed commit IDs and authors without contents or keys. Isolate failed documents, fairly resume large histories and prioritise current edits over historical bodies.

## Compatibility and limits

Upgrade **all communicating devices to V1.5.5**. Older protocol peers are refused. Do not downgrade migrated profiles. Device identities, encryption and the update-signing key are unchanged.

Migration does not silently delete available data. It cannot recreate bytes deleted before upgrading or recover an encrypted profile without its key. Unavailable peers, corrupt data, full disks and quotas can still pause sync; recovery paths remain available. Explicit cleanup can remove retired sync-only history and retains its confirmation. Keep a separate backup before upgrading important work.

Membership, revocation, target consent, external-file checks and autosave remain enforced. No promise of error-free networking or automatic text conflict merging.

Source review and native compilation/packaging only. **No automated, interactive or installation tests; no app launches.** Windows/Apple developer certificates remain unconfigured; update metadata uses the existing Ed25519 key. Original code remains under The Unlicense.

See [Sync history and recovery](https://github.com/OnlySecci/EditorTool/blob/main/docs/sync-history.md).
