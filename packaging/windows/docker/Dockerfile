FROM rust:1.95-bookworm@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates \
        curl \
        file \
        g++-mingw-w64-x86-64-posix \
        gcc-mingw-w64-x86-64-posix \
        nsis \
        openssl \
        p7zip-full \
        shellcheck \
        unzip \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add clippy rustfmt \
    && rustup target add x86_64-pc-windows-gnu

WORKDIR /work
