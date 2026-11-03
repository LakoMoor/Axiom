# Changelog

All notable changes, newest first. Each line says what the person notices.

## Unreleased

- Interface: Settings and Agent fit a phone's screen (the settings tabs wrap along the top and each setting's title sits above its control).
- Interface: a phone layout for narrow screens (and always on Android): no title bar or side panel, navigation along the bottom, the welcome, prompt box and tiles fit a narrow screen, and arrows now draw in the app's font.
- Android: an app for phones (`apps/axiom-android`), the same interface in a native activity, built as an APK by `packaging/android/package.sh`. It starts on the Android 14 emulator; a phone layout, a file picker and local models there are still to do.
- Builder: when a project is ported to Rust the agent lays it out as a Cargo workspace (`crates/`, `apps/`, `docs/`, `scripts/`), the way a well-kept Rust application repository does.
- Repository: the licence-bundle config moved to `release/`; added `GOVERNANCE.md`, `TRADEMARKS.md`, `CODEOWNERS` and `dependabot.yml`.
- Interface: a Build / Agent switch on top of the sidebar, a profile card at its foot, and an "Update available" pill in the title bar when a newer release is out (one anonymous check per start).
- Clean room: the Strata tools and the update check work in Clean room mode (they touch no target).
- Models: Strata can install and start a ready-made big local model in one click (tools `models_strata_*`).
- Interface: liquid-glass, rounded windows, an icon rail, macOS window buttons.
- Agent mode: give an agent app a folder and a task from inside Axiom.
