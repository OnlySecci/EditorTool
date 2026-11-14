# EditorTool development

- The owner requested a public GitHub repository for V1.0. Keep it public unless the owner changes this.
- Use feature branches and pull requests for follow-up work; releases come from version tags on main.
- The owner requested builds and packages without automated or interactive tests. Do not add or run test suites unless the owner changes this preference.
- Preserve the native Rust/FLTK architecture, event-driven idle behavior and plain-text clipboard output.
- Never commit session caches, personal documents, credentials, generated installers, build output or the temporary toolchain.
- Update Cargo.toml, packaging metadata and the changelog together for a release.
- Build native binaries for x64 and ARM64 on each target operating system. Windows installers use the NSIS cross-compiler on Linux, Linux uses Flatpak/RPM/DEB, macOS uses DMG.

- Maintain repository documentation, code identifiers/comments, pull requests and GitHub release notes in English. User-facing translations belong in the embedded language catalogs.
