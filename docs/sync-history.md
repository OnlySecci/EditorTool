# Sync history and recovery (V1.5.5)

## Decisions follow content ancestry

No permanent master device exists. Immutable commits identify their approved group, author, contents and actual content parents. Separate counter provenance prevents author-counter reuse without falsely claiming discarded contents were merged. Clocks and legacy force generations are not winner rules.

| Situation | Result |
| --- | --- |
| A edits; B receives it; B edits that revision | B's descendant fast-forwards A and C after the writing pause. |
| C is offline and unchanged while A/B work | C follows the current descendant when connected. |
| Only C changes an offline copy | Its descendant becomes current everywhere. |
| A/B continue a branch while offline C edits an older base | Both branches remain. A user chooses or merges. |
| All three edit independently | All known tips remain; no timestamp guessing. |
| Concurrent branches contain identical bytes | A stable participating identity merges them, or the next edit joins identical tips. |
| A user selects a displayed branch | A signed resolution joins the confirmed heads with the chosen contents. |
| The user combines text/images | Use editor text resolves with those reviewed contents. |
| A branch appears after the view opened | Refresh the decision rather than silently including unseen work. |
| Disconnected devices resolve differently | Their decisions remain branches. Decide after both are known. |
| Chosen contents already exist locally | Authenticate the body and still apply the new commit metadata. |

The common main is a commit, not a device. A decision includes only known confirmed heads. Devices converge once they have the same verified graph, contents and user decision, subject to permissions, connectivity and quotas. This is not a guarantee of error-free operation.

## Version view

Open Document versions from the document menu/context menu. Branches and history show commit IDs, type, author and content parents. Preview, compare, open an unsynchronised recovery copy, select Use selected as main, or edit the shared document and choose Use editor text. Unsent edits are first preserved as branches; if that changes the heads, review the refreshed list before confirming.

Linked old groups appear as ARCHIVE while their local records remain retained. Available bytes may be opened or compared. Archived/unverified records cannot directly become signed parents of another group: copy/merge their contents into the shared editor and choose Use editor text. Recovery copies do not overwrite original files. Missing historical bytes are requested from approved devices; bytes deleted everywhere cannot be recreated.

## Existing links

Before migration, the unlocked profile stores encrypted copies under `session/recovery-v155` and `sharing/recovery-v155`. Available source files are not silently removed. Backups use the profile vault and require its master key/recovery mechanism. Keep a separate profile backup and recovery key before upgrading; never upload these to issue reports.

Old signatures are revalidated. Exact immutable base IDs replace ambiguous clock/hash-only links. Known branches hidden by older force handling are reconstructed. Invalid records remain quarantined. Independent encrypted pending-draft records prevent newer drafts being replaced by an older session cache after interrupted writes.

A missing base triggers scoped lookups against approved active members. Once found, editing resumes on that base. If all eligible members report it unavailable, preserved text becomes an independent signed recovery branch anchored to the root, with no invented content parent. Offline members can keep lookups pending; Repair document sync offers Keep as branch.

If the root/link is absent, ask already paired devices for signed public history metadata. Both devices must be document members; a returned root needs local confirmation. Alternatively start a new history and select recipients. They must accept; different local copies remain branches. A signed replacement reference permits relinking corresponding old tabs, including after restart. Automatic file saving needs fresh approval for the new link. Participant changes always use a new group.

## Background recovery and limits

Proofs transfer in batches of up to 32 with a resumable 512-node round budget. Metadata paging and document scheduling prevent one long history blocking every document. Current heads take priority over historical bodies. Negative receipts avoid repeatedly requesting unavailable data from one peer; reconnect or manual retry permits another attempt.

Available immutable history remains referenced while its group is retained. This is a bounded full-snapshot store, not unlimited Git storage or text auto-merge. Document/author/global/proof quotas and the metadata ceiling still apply. Quota/integrity/storage failures pause work without silently pruning branches. Adjust limits, repair storage, manually save recovery copies or start a new approved history as appropriate.

Explicit cleanup can remove retired groups and unreferenced sync-only records; review its confirmation first. Upgrade backups remain separate. Damaged snapshot repair archives original bytes and authenticates the replacement before an atomic swap. If backup/persistence fails, replacement stops.

Revoked/departed authors are not reactivated by restoration. Already accepted records may remain visible; sent data cannot be recalled. Root proposals containing revoked participants are refused: create a new group without them. Membership, private-network addressing, TLS, target approval and external-file checks remain enforced.

Full logs contain commit/group/author identifiers and recovery labels, never bodies or keys. Pseudonymised export omits these added identifiers. An acknowledgement describes known metadata, not proof that every offline device applied it or that storage is healthy.
