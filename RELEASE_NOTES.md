# EditorTool V1.5.6

## macOS clipboard and application lifecycle

- Consume clipboard and shortcut events before native defaults, preventing the same paste from being inserted repeatedly by both handlers.
- Route native Quit requests (including Command-Q) through the application-wide save/flush and shutdown path. Explicit quitting overrides background/tray mode; closing a window keeps its separate behavior.
- Handle Dock reopen requests by showing existing windows or creating a window when needed.
- Confirm file/window handoff only after the running instance processes it. Retry session acquisition briefly when the previous instance is shutting down. Never delete a live instance's lock or clear document caches as a startup workaround. Add diagnostic events for lifecycle and handoff failures.

The reported restart failure after Force Quit was not reproduced because no app launches or tests were requested. These changes address the identified quit/reopen/handoff paths. Force Quit can only recover the last completed durable session write.

## Immediate sync

A **Sync now** button sits beside the document sync countdown. It publishes the current changes and wakes communication immediately instead of waiting for the writing pause. It does not force a version winner, bypass conflicts, grant access, or make offline devices available. Existing main-version decisions remain in Document versions.

## Pinned tabs and groups

Pin/unpin tabs and assign named groups from the File menu or tab context menu. Pinned tabs appear first; tabs in the same named group appear together with a group label. Use an empty name or Remove from group to clear membership. Pin state, group names and the invisible-character display preference survive session restoration and are local to the editor, not shared document contents.

## Readable communication log

The default view shows plain-language connection checks, sharing requests, transfers, confirmations, conflict/recovery events and actionable problem explanations. Device filters and search remain available. **Show technical details and background messages** reveals protocol traffic, endpoints and commit identifiers. The filtered export follows the selected view; pseudonymised export retains its separate privacy rules. A confirmation is not a claim that every offline device is current.

## Optional text cleanup

Choose **Edit > Clean up text** to remove trailing spaces/tabs, reduce consecutive empty lines to one, or apply both. Indentation and images are retained. Cleanup is explicitly invoked and supports Undo/Redo. Mixed documents retain text-only cleanup snapshots and recalculate image offsets without duplicating encoded image payloads. A subsequent document load or incoming replacement resets that cleanup context.

**View > Show invisible characters** displays spaces, tabs and line breaks as an overlay. It does not insert symbols into the document, clipboard or sync data. Rendering is limited to visible text with a fixed work budget.

## Automatic unlock for installed profiles

Existing password-protected, non-portable profiles can move their existing master key to the system key store after a successful password unlock. A one-time offer and **Settings > Security > Use system key store** provide the conversion. Verify storage before switching; preserve a password-wrapped fallback header and retain the current protection if conversion fails. Documents, history, device identity and sync associations use the same key and remain intact.

Linux now supports native KDE Wallet 6/5 as a fallback when Secret Service storage cannot be used, including scoped Flatpak D-Bus permissions. This addresses KDE/Nobara installations without a usable Secret Service endpoint; it cannot unlock a disabled or unavailable system wallet without OS cooperation. The OS can still request its own wallet unlock/access approval. Portable profiles retain password protection. Key-store failures are logged without password/key values. A previously converted profile can use its preserved password header for a session if OS key storage is unavailable.

## Compatibility and packages

The sync wire protocol remains compatible with V1.5.5. Do not downgrade profiles that now use the new KDE Wallet storage mode. Original saved files keep their existing formats. Update metadata uses the existing pinned Ed25519 trust key; Windows developer signing and Apple notarisation remain unconfigured.

Source review, native compilation and packaging only. **No automated, interactive or installation tests, no application launches and no screen control.** All six native platform targets and all 16 package formats are required for publication. Original code remains under The Unlicense.
