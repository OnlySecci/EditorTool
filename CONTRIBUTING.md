# GitHub workflow

The canonical public repository is https://github.com/OnlySecci/EditorTool.

1. Branch from `main`, for example `feat/image-flow` or `fix/tab-scroll`.
2. Update source, version/packaging metadata, documentation, changelog and English release notes together. Retain third-party notices.
3. Push and open a pull request. Actions builds all six OS/architecture combinations and creates the Windows setup executables. At the owner's request, do not run automated, interactive or installation tests, or launch the app.
4. Review and merge after required builds finish. CODEOWNERS assigns the repository owner; administrators retain bypass access, so this is not a claim of independent two-person approval.
5. From **main**, dispatch **Build and release** with `release_tag=V<version>`. Do not publish by pushing a tag: the protected signing environment only accepts main. The workflow creates the release tag at its built commit.
6. Confirm all 16 installers/archives and three verification files are published, then verify the signature, package hashes, sizes and exact inventory.

The `release-signing` environment contains `UPDATE_SIGNING_KEY_B64`; it permits only main deployments. Build jobs and pull requests do not receive that secret. Renewal uses trusted main code and authenticates old metadata and original packages before re-signing. See [update signing](docs/update-signing.md). Administrators and anyone able to modify trusted signing code remain in the security boundary.

Release downloads are public. Windows developer certificates are not configured. macOS bundles receive an ad-hoc signature and are not notarised. Update metadata signing is independent of platform code signing.

Documentation, code identifiers/comments, pull requests and release notes are English. UI translations belong in embedded catalogs. Preserve Rust/FLTK, low idle resource use, plain-text clipboard output and explicit sharing consent. No private keys, tokens, caches, documents, installers or toolchains belong in Git. Original contributions use The Unlicense; third-party licence terms remain unchanged.
