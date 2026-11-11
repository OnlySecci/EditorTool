# Changelog

## V1.4.5 — 2026-11-11

- Apply sync revisions in place, reuse image frames/decoded assets, and preserve viewport, caret, selection and focus.
- Stable mixed-document wrap measurement without transient inner vertical scrollbars.
- Persist image-only revisions without losing cached image/size changes.
- Visible five-second sync countdown and per-peer revision confirmation status.
- Themed, grouped tab popup with language/split subpages and keyboard navigation.
- Build/package only; no tests run, as requested.


## V1.4.1 — 2026-11-11

- Settings menu next to Edit, with general/client settings embedded as document-list tabs.
- Labeled client fields and connection-list columns; styled device/document consent sheets.
- Focus-only caret blinking and persistent 50–300% text zoom.
- Native font metrics for mixed-document spacing, fixing growing image gaps on macOS.
- Durable document-specific sync departure notifications with remaining group members preserved.
- Required TLS 1.3, AES-256-GCM and hybrid X25519MLKEM768 via Rustls/AWS-LC; classical pinned device signatures retained.
- All sharing peers must update to V1.4.1 or newer; existing identities/data are retained.
- Build/package only; no tests run, as requested.


## V1.4 — 2026-11-11

- Opt-in private-network document transfer and multi-peer sync over pinned mutual TLS.
- Configurable local/client TCP ports, client status and explicit device/document consent.
- Five-second edit debounce, persistent outbox/inbox and offline reconciliation.
- Causal device counters, content hashes and manual resolution of divergent versions.
- Per-document local autosave with external-file change protection.
- Optional native system tray continuation on Windows, Linux and macOS.
- Build/package only; no tests run, as requested.


## V1.3 — 2026-11-11

- Proportional image resizing with four draggable corner handles.
- Optional automatic insertion at up to half the text area width.
- Tab context menu with save, language and tab-management actions.
- Independent split panes side by side or stacked, with their own tabs and resizable divider.
- Persisted split layout and image display widths; detach splits without discarding documents or undo state.
- No tests run, as requested.


## V1.2 — 2026-11-11

- Built-in GitHub update checks with release notes and manual installation.
- Optional startup checks and opt-in automatic startup installation.
- Bounded background downloads, SHA-256 verification, session-safe installer handoff and restart.
- Native installation paths for setup/portable Windows, macOS bundles, Linux packages and Flatpak.
- No tests run, as requested.


## V1.1 — 2026-11-11

- Configurable word wrap, color schemes, accent colors and editor font size.
- Close all tabs with save/cancel handling.
- Persisted image display size and image-anchored paste scrolling.
- Rounded visible scrollbars, edge-aligned editor and removal of the duplicate menu logo.
- No tests run, as requested.


## V1.0 — 2026-11-11

- Native Rust/FLTK editor for Windows, Linux and macOS, each for x64 and ARM64.
- Compact dark interface, multiple windows and tabs, horizontal or vertical tab arrangement.
- Drag the divider to resize the vertical tab list; its width survives a restart.
- Saved tabs use the filename; unnamed tabs use the first words of their text.
- Text and images in one sequential document, insertion from file or image clipboard.
- `.etdoc` format embeds images; plain text and source files retain normal file formats.
- Plain-text clipboard cleanup, line numbers, text/selection counts, search and replace, text comparison.
- Python, PowerShell, Rust, JavaScript/TypeScript and other lexical syntax colors.
- Automatic session recovery, including unnamed text, images, comparison text and cursor positions.
- File associations and file-open forwarding to an existing editor instance.
- Public GitHub repository, multi-architecture package pipeline and release checksums.
- Windows setup EXE, Linux Flatpak/RPM/DEB, macOS DMG and portable archives.
- The project's own code is released under The Unlicense; dependency licenses are included separately.
