# GitHub workflow

The canonical repository is private: https://github.com/OnlySecci/EditorTool.

1. Branch from `main`, e.g. `feat/image-flow` or `fix/tab-scroll`.
2. Commit the source change and update the relevant documentation.
3. Push the branch and open a pull request. GitHub Actions builds Windows, Linux, macOS Intel and macOS Apple Silicon. No tests are run.
4. Merge the pull request after review.
5. Update `Cargo.toml` and `CHANGELOG.md`, commit, and tag the release with `V<version>`.
6. Push the tag. The release workflow builds installation packages, collects SHA-256 checksums, and publishes a GitHub Release in the private repository.

Release downloads remain accessible only to accounts with repository access. Installers are unsigned until platform signing identities are supplied. macOS bundles receive a local ad-hoc signature; they are not notarized.

Build instructions are in README.md. Package scripts are in `scripts/` and `packaging/`. The GitHub Actions workflow uses repository-scoped `GITHUB_TOKEN` access; no personal token is stored in this project.

The first release tag is `V1.0`, corresponding to package version `1.0.0`. Releases can also be triggered using the workflow input `release_tag`; publishing creates that tag at the built commit. All six native builds and the Windows installer job must succeed first.

The project uses The Unlicense. Submit original contributions under the same terms and retain third-party notices.
