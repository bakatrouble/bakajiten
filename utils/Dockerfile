# syntax=docker/dockerfile:1

# Data-pipeline image: builds the `utils` binaries (fetch-data,
# pack-data) and runs them anywhere with network + disk. Builder
# uses rustup (Alpine's packaged Rust lags the workspace's
# toolchain floor); the runner is bare Alpine + certs, since the
# musl binaries link statically. Outputs (*.bin) are
# platform-independent.
#
# Local build:
#   docker build -f utils/Dockerfile -t bakajiten-utils .
#
# Fetch upstream sources (bulk, gitignored):
#   docker run --rm -v "$PWD:/work" bakajiten-utils \
#       fetch-data --out-dir /work/dataimports
# Pack artifacts (verified at write time):
#   docker run --rm -v "$PWD:/work" bakajiten-utils \
#       pack-data --kanjidic /work/dataimports/kanjidic2.xml --kanji-out /work/data/kanji.bin [...]
FROM alpine:3.22 AS builder
RUN apk add --no-cache curl gcc musl-dev ca-certificates

ENV RUSTUP_HOME=/opt/rustup CARGO_HOME=/opt/cargo PATH=/opt/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
RUN --mount=type=cache,target=/opt/cargo/registry <<EOF
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --default-toolchain stable --profile minimal
rustc --version
EOF
WORKDIR /work

# Manifests + sources together: the lib crates use non-standard
# layouts, so manifests alone don't validate. The cargo target dir
# is a cache mount (never lands in the image), so rebuilds stay
# incremental across image builds.
COPY Cargo.toml Cargo.lock ./
COPY lib/ lib/
COPY utils/ utils/
RUN --mount=type=cache,target=/opt/cargo/registry --mount=type=cache,target=/work/target \
    cargo build --release -p utils --bins \
 && mkdir -p /out \
 && cp /work/target/release/fetch-data /work/target/release/pack-data /out/

FROM alpine:3.22 AS runner
RUN apk add --no-cache ca-certificates
COPY --from=builder /out/fetch-data /out/pack-data /usr/local/bin/

WORKDIR /work
ENTRYPOINT []
CMD ["pack-data", "--help"]
