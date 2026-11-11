# Changelog

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
