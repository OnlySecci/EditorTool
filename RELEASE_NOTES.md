# EditorTool V1.5.4

## Force sync main

Select the desired revision in a document's version/conflict view and choose **Force sync main**. A signed logical force generation makes that content the common main version. Older generations and ordinary offline edits based on them cannot replace it. Later edits based on the winning main version can sync normally.

Concurrent force decisions use the same deterministic ordering on every device. Device clocks are not used; this cannot establish the real-world chronology of decisions created while disconnected. After learning a decision, another explicit force action advances to a higher generation. Concurrent ordinary edits within the same winning generation still require conflict resolution.

Unsent local edits are preserved in a separate unsynchronised recovery tab and the protected session is flushed before the shared tab is replaced. If recovery cannot be persisted, replacement is refused. Superseded published heads remain subject to the existing bounded history. A restart reconciles separately persisted main decisions with older cached tabs.

Communication logs record force creation, application, ignored superseded peers, acknowledgements and recovered drafts. Full logs show group, generation and deciding fingerprint; the new SYNC filter and text search help locate these events. Pseudonymised export excludes the added group/authority identifiers. An acknowledgement is not a claim that every offline client has applied the decision.

## Compatibility and safeguards

Upgrade all communicating devices to the compatible V1.5.4 protocol. V1.5.3 groups and signatures are preserved; older peers are refused because they cannot enforce force-main authority. Do not downgrade a profile after using the feature. Existing recovery-key protection and the update signing trust key are unchanged.

The implementation preserves a losing device's signed author-counter history when editing the new main version, avoiding counter reuse. Equal-content concurrent revisions remain separate signed records instead of mutating signed vector clocks in place. Force never bypasses document consent, revocation, quotas, executable-target approval, autosave preferences or external-file conflict checks.

See [Force sync main](https://github.com/OnlySecci/EditorTool/blob/main/docs/force-sync-main.md) for the design and convergence conditions. Source review and native builds/packaging only; no automated, interactive or installation tests and no app launches. Platform developer signing certificates remain unconfigured; update metadata uses the existing Ed25519 trust key. Original code remains under The Unlicense.
