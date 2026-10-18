#!/bin/sh
# Native macOS build + test. Run from the repo root:
#   ./frontends/frontend_qt/packaging/build-macos.sh [clean]
# Requires: Xcode command-line tools, Qt 6.10.3 (via aqt to pin
# the version, exposed through Qt6_DIR), yaml-cpp, Rust,
# uniffi-bindgen-cpp. Configures in
# frontends/frontend_qt/build-macos/builddir, runs the offscreen test
# suite, and bundles a self-contained
# frontends/frontend_qt/build-macos/Bakajiten-macos-<arch>.zip
# (Bakajiten.app inside, Qt frameworks included) for the host
# architecture (no universal binary: the Rust crates build
# single-arch through Corrosion, so a universal app would need
# lipo'd Rust staticlibs first).
set -e
cd "$(dirname "$0")/../../.."
QTAPP="$PWD/frontends/frontend_qt"
if [ "$1" = "clean" ]; then
    rm -rf "$QTAPP/build-macos"
    exit 0
fi
for tool in cmake ninja cargo rustc uniffi-bindgen-cpp; do
    command -v "$tool" >/dev/null || {
        echo "missing: $tool" >&2
        exit 1
    }
done
case "$(uname -m)" in
    arm64) ARCH=arm64 ;;
    *) ARCH=x64 ;;
esac
mkdir -p "$QTAPP/build-macos"
cmake -S frontends/frontend_qt -B frontends/frontend_qt/build-macos/builddir -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build frontends/frontend_qt/build-macos/builddir --target bakajiten test_mainwindow
(cd frontends/frontend_qt/build-macos/builddir && QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 ctest)
"$(dirname "$0")/bundle-macos-app.sh" \
    frontends/frontend_qt/build-macos/builddir/bakajiten "$QTAPP/build-macos/Bakajiten-macos-${ARCH}.zip"
