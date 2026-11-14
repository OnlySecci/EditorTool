# EditorTool V1.5.2

## English project documentation

The repository description, documentation, source wording and release notes are maintained in English. Historical release descriptions have been translated without changing their tags, packages or signatures. The README has a centred title, logo and badges, a concise project introduction, download/support/contribution links, feature overview, screenshot and platform build instructions. Historical changes remain in the changelog and release pages.

## Interface languages

Settings → General → Interface language offers English, German, French, Italian, Spanish, Portuguese, Dutch, Polish, Turkish and Ukrainian. The choice is saved for all windows. Restart EditorTool to apply it; changing the preference does not close documents or restart the app automatically. Existing settings without a language preference default to English.

The initial catalogs cover menus and common editing, file, search, comparison, sharing and settings controls. Detailed technical messages and untranslated text fall back to English; this is not a claim of complete translation of every message. Catalogs are embedded and require no translation service or network requests. Document text, filenames, paths, client names, keys, protocol identifiers and syntax-language choices are not translated. Release notes remain in English regardless of UI language.

Translated menus use the same paths for custom shortcut lookup, and translation text cannot introduce unexpected submenu separators. The native Rust/FLTK architecture and event-driven idle behaviour remain in place.

## Packages and validation

All 16 platform packages: Windows setup EXE/portable ZIP; Linux Flatpak/RPM/DEB/portable TAR.GZ; macOS DMG/app ZIP; each for x64 and ARM64. Also included: SHA256SUMS.txt, update-metadata.json and update-metadata.sig, using the existing V1.5.0 Ed25519 trust key.

Source review, native release builds and packaging only. No automated, interactive or installation tests and no app launches, as requested. No Windows/Apple developer certificates; macOS bundles are ad-hoc signed and not notarised. The Unlicense applies to original code; dependency licences remain unchanged.

## Scope

V1.5.1's long-path fixes, interface selection and network diagnostics remain included. Transport remains TLS over TCP. UDP/QUIC preference with protected TCP fallback and further device-key security review are planned for V1.5.3 and are not part of this release.
