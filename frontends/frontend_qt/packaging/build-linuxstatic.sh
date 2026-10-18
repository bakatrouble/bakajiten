#!/bin/sh
# Static Linux build. Run from the repo root:
#   ./frontends/frontend_qt/packaging/build-linuxstatic.sh [clean]
# Configures in frontends/frontend_qt/build-linuxstatic/builddir and
# produces
# frontends/frontend_qt/build-linuxstatic/Bakajiten-linux-x86_64.tar.gz
# (bakajiten inside): one file with Qt linked statically (system
# glibc/XCB stay dynamic — fully static glibc breaks DNS/TLS at
# runtime, so no `-static` link here, unlike the Windows cross
# build). The build tree is container-owned (root); `clean` wipes it
# from inside the container (host rm fails on it). First run builds
# the image (Qt, ~40-60 min, cached in its own layer):
#   docker build -f frontends/frontend_qt/packaging/docker/Dockerfile.linux-static \
#     -t bakajiten-cross:linux-static .
set -e
cd "$(dirname "$0")/../../.."
QTAPP="$PWD/frontends/frontend_qt"
if [ "$1" = "clean" ]; then
    docker run --rm -v "$QTAPP:/qtapp" bakajiten-cross:linux-static \
        sh -c 'rm -rf /qtapp/build-linuxstatic'
    exit 0
fi
mkdir -p "$QTAPP/build-linuxstatic/builddir"
docker run --rm -v "$PWD:/work" -v "$QTAPP/build-linuxstatic/builddir:/build" \
    bakajiten-cross:linux-static \
    sh -c 'export HOME=/root; cmake -S /work/frontends/frontend_qt -B /build \
        -DQt6_DIR=/opt/qt6-static/lib/cmake/Qt6 \
        -Dyaml-cpp_DIR=/opt/yaml-cpp-linux/lib/cmake/yaml-cpp \
        -DOPENSSL_ROOT_DIR=/opt/openssl \
        -DBUILD_QT_TESTS=OFF \
        && cmake --build /build'
rm -f "$QTAPP/build-linuxstatic/Bakajiten-linux-x86_64.tar.gz"
tar -czf "$QTAPP/build-linuxstatic/Bakajiten-linux-x86_64.tar.gz" -C "$QTAPP/build-linuxstatic/builddir" bakajiten
