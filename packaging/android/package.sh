#!/usr/bin/env bash
# Build the Android app:  $DIST/axiom-<version>-android-arm64.apk
#
# Usage: packaging/android/package.sh
#
# Needs: the Android SDK (ANDROID_HOME) with an NDK (ANDROID_NDK_HOME), build-tools, `cargo-apk`
# (cargo install cargo-apk --locked) and the aarch64-linux-android Rust target. The APK is signed
# with the key in AXIOM_ANDROID_KEYSTORE (password AXIOM_ANDROID_KEYSTORE_PASSWORD); without
# them it uses the debug key every Android toolchain makes, a throwaway one: such an APK can't be
# updated in place by a build from another computer, and it is not for a store.
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"

: "${ANDROID_HOME:?set ANDROID_HOME to the Android SDK}"
: "${ANDROID_NDK_HOME:?set ANDROID_NDK_HOME to an NDK inside the SDK}"
command -v cargo-apk >/dev/null || { echo "error: cargo-apk not found (cargo install cargo-apk --locked)" >&2; exit 1; }

KEYSTORE="${AXIOM_ANDROID_KEYSTORE:-$HOME/.android/debug.keystore}"
[ -f "$KEYSTORE" ] || { echo "error: no keystore at $KEYSTORE (set AXIOM_ANDROID_KEYSTORE)" >&2; exit 1; }
export CARGO_APK_RELEASE_KEYSTORE="$KEYSTORE"
export CARGO_APK_RELEASE_KEYSTORE_PASSWORD="${AXIOM_ANDROID_KEYSTORE_PASSWORD:-android}"

(cd "$ROOT" && cargo apk build -p axiom-android --target aarch64-linux-android --release)

APK="${CARGO_TARGET_DIR:-$ROOT/target}/release/apk/Axiom.apk"
[ -f "$APK" ] || { echo "error: $APK missing after the build" >&2; exit 1; }
mkdir -p "$DIST"
OUT="$DIST/axiom-$VERSION-android-arm64.apk"
cp "$APK" "$OUT"
echo "wrote $OUT ($(wc -c < "$OUT") bytes)"
