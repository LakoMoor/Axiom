# Releasing Axiom

The release recipe lives in the shared workflows of [axiom-actions](https://github.com/LakoMoor/axiom-actions);
this page is all you need to cut one. In short: a release is whatever is on the `release` branch. Pushing to it makes
`.github/workflows/release.yml` build, sign and package every platform and attach the files to a
draft GitHub Release, which a maintainer publishes once every job is green. Signing needs the
maintainers' certificates (secrets of the `release` environment, which only the `release` branch can read), so a fork's run produces unsigned packages.

`ci.yml` checks every pull request and every push to `main` once the repository is public; while
it's private its jobs are skipped (they would cost Actions minutes), so run it by hand before a
release: `gh workflow run ci.yml --ref main`. The other workflows (`freebsd.yml`,
`windows-arm64.yml`, `packaging-lint.yml`) run by hand (`gh workflow run <file>`).

## Cutting a release

1. Bump the version on `main`: `cargo xtask version set X.Y.Z` (the workspace version in the root
   `Cargo.toml` is the single source; `cargo xtask version` prints it).
2. Fast-forward `release` to `main`. The push runs `.github/workflows/release.yml`, which builds every
   platform and creates or updates the draft GitHub Release **Axiom vX.Y.Z**.
3. When every job is green, publish the draft.

A dry run of the unsigned platforms from any branch: `gh workflow run release.yml --ref <branch>`.

## What it ships

| Platform | Files |
|---|---|
| macOS | `axiom-<v>-macos-universal.dmg` (`Axiom.app`), `axiom-cli-<v>-macos-universal.zip` |
| Windows | `axiom-<v>-windows-{x64,x86,arm64}.msi` and `-portable.zip` |
| Linux | `axiom-<v>-linux-{x86_64,aarch64}.{AppImage,AppImage.zsync,deb,rpm,tar.gz,flatpak}`, `axiom-<v>-linux-riscv64.tar.gz` |
| FreeBSD | `axiom-<v>-freebsd-x86_64.tar.gz` |
| Web | `axiom-web-<v>.zip` (the browser viewer, `apps/axiom-web`) |

Plus `SHA256SUMS.txt`.

## App-specific choices

- **No file associations.** Axiom opens any file, so it claims no file type and no URL
  scheme on any platform. It appears in Windows' "Open with" list and accepts paths on the
  command line. Projects (`*.cany`) are folders.
- **The macOS executable** is `Axiom.app/Contents/MacOS/axiom`. Only the bundle
  carries the display name. `axiom-cli` sits beside it, as it does in the Windows zip: the
  app starts it as the coding agents' MCP server, so an app without it can't build (#91).
- **Windows portable mode:** the portable zip ships `portable.txt`. While it sits next to the exe,
  settings live in `AxiomData\` (`crates/engine/src/settings.rs`).
- **Flatpak** grants `--share=network` (AI backends, engine downloads) and `--filesystem=home:ro`
  (files to inspect, passed by path).
- **Fonts** are committed in `assets/fonts` (OFL-1.1). Release and CI builds also embed axiom-fonts
  at the commit pinned in `release.yml` and `ci.yml` (`AXIOM_FONTS_DIR`, `AXIOM_FONTS_REQUIRED=1`),
  the fallbacks for Korean, Chinese, Japanese, Thai, Arabic, Hebrew, Devanagari and symbols; a
  build without them shows those as boxes (#19). Every package ships all their licences
  (`OFL-*.txt`, axiom-fonts' as `OFL-<font>.txt`).
- **The WiX UpgradeCode** `8F69A8B5-4384-479B-B3FF-93C5374BCA51` belongs to Axiom for good.
