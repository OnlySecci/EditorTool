# Force sync main

Open a synchronised document's version/conflict view, select the revision whose contents should win, then choose **Force sync main** and confirm. This creates a signed decision, not a permanent primary device. Subsequent ordinary edits based on the chosen main version can synchronise normally.

## Agreement across devices

Every decision carries a signed force marker: a logical generation, deciding device fingerprint, random decision nonce and the exact selected signed parent revision. Its generation is one greater than the maximum authenticated parent generation. The selected parent's content hash and size must match the forced revision. Normal descendants must inherit their parents' greatest force marker exactly; dropping it or inventing an arbitrary generation is rejected.

A higher generation supersedes older generations and ordinary offline edits based on them, even when their ordinary revision counters are larger. Two independently created decisions with the same generation are ordered identically everywhere by device fingerprint, then nonce, then selected revision ID. This tie-break is deterministic, not a claim about chronological recency or which content is better. Computer clocks are not consulted. A later force action made after learning a decision advances beyond it; disconnected devices cannot know an unseen decision's real-world time.

Devices validate the existing signature and causal-parent chain before accepting a marker. The force ordering is also used when deciding which bodies to fetch/send, so old offline content cannot overwrite a newer main version through the transfer path. A losing device retains its own signed author-counter provenance when publishing subsequent edits, avoiding counter reuse after adopting a main revision that did not know its discarded branch.

Given eventual connectivity, compatible authorised clients, available proof/body data and sufficient quotas, devices converge to the greatest force decision and its subsequent revisions. Concurrent ordinary edits within that same winning generation still produce a normal conflict: choose the desired contents and force again. There is no central server, permanent administrator device or guarantee of immediate agreement with an offline client.

## Preservation and saving

Locally edited text that has not yet been published is copied into a separate, unsynchronised recovery tab before an incoming force decision replaces the shared tab. The protected session containing that recovery copy is flushed before replacement. If protected session recovery is unavailable or the flush fails, the local replacement is refused. A restart reconciles a cached tab with a newer separately persisted force decision. Existing quota-limited revision history retains superseded published heads where available; history is not an unlimited backup.

Force applies to the shared editor document. It does not bypass a device's autosave preference, target approval for code/startup files, external-file conflict detection, revoked identities, document membership, storage quotas or signed-proof limits. A local disk-save failure is reported and does not prevent other open copies from adopting the main revision. Keep original-file backups where needed.

## Logs and compatibility

Communication logs record decision creation, application, ignored superseded peers, revision acknowledgements and recovered local drafts under **SYNC**. Full logs include group ID, generation and authority fingerprint; filter by SYNC, search for `Force sync main`, or filter by a device. An acknowledgement describes a protocol exchange, not proof that every offline device has applied the decision. Pseudonymised export omits group IDs and authority fingerprints as well as the existing endpoint/identity fields; timing, event labels and generation numbers remain visible.

All participating devices must use the compatible V1.5.4 sync protocol. Older peers are refused rather than allowed to discard the new authority marker. Existing V1.5.3 signatures and groups are preserved during upgrade; older pre-signed-protocol groups remain archived. Do not downgrade a profile after using force decisions. The implementation also stops merging signed vector clocks in place for equal-content concurrent versions; such mutation invalidated their signatures. These now remain distinct signed revisions until resolved.

Validation is source review and native compilation/packaging only. No app launches, automated tests, interactive tests or installation tests are performed under the project owner's instructions.
