# Interface languages

The application embeds JSON catalogs from `locales/`. English is the source and fallback language. The current choices are English, German, French, Italian, Spanish, Portuguese, Dutch, Polish, Turkish and Ukrainian.

Select a language in Settings → General → Interface language, then restart the application. The preference is stored in the existing settings file. Loading one selected catalog is a startup operation; there is no translation service, polling or network access. A language change does not automatically restart the application or close documents.

Initial translations cover menus and common controls. The German catalog also retains many earlier messages. Detailed technical errors and missing entries use English. This release does not claim complete translation coverage of every message. Documentation and release notes remain English.

## Updating a catalog

- Use the exact English source phrase as the JSON key. Keep identifiers and source comments in English.
- Never translate user documents, clipboard contents, filenames, paths, client names, protocol values, serialized command identifiers or cryptographic material.
- Preserve the names and counts of format placeholders. The formatter first renders arguments in Rust, then substitutes opaque values into the translation, so inserted user text is not translated or treated as a formatting instruction.
- Menus translate each hierarchy component. Literal slashes in translated components are displayed as separators rather than interpreted as additional submenus. Menu construction and keyboard-shortcut lookup must use the same translation function.
- Keep language names in their own language so users can recover from an accidental selection. Stable language indices in `src/i18n.rs` must not be reordered without migrating saved preferences.
- Provide an English fallback when a message or catalog is unavailable. Do not turn translations into an external executable or template language.

Current verification is source review and native compilation/packaging only, as requested by the owner. Do not launch the app or add/run tests without a change to that instruction.
