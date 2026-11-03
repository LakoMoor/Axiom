# Governance

Axiom is maintained by its maintainers (the owners of this repository). They set the direction,
review and merge pull requests, and cut releases.

- **Decisions:** small changes are decided in the pull request. Anything marked **decision** in
  `docs/ROADMAP.md` waits for a maintainer.
- **Contributions:** everyone may take an unclaimed roadmap item (see `AGENTS.md`, "Working rules").
  Pull requests are squash-merged into `main` after the checks pass.
- **Releases:** a maintainer tags a version (`cargo xtask version set X.Y.Z`), the release workflow
  builds the packages and drafts the GitHub release, and a maintainer publishes it.
- **Security:** report problems as described in `SECURITY.md`.
