FROM rust:1.95-bookworm@sha256:6258907abe69656e41cd992e0b705cdcfabcbbe3db374f92ed2d47121282d4a1

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        ca-certificates curl file libpulse-dev libxkbcommon-dev libxkbcommon-x11-0 libwayland-dev \
        libgl1-mesa-dev libegl1-mesa-dev libvulkan1 mesa-vulkan-drivers \
        libx11-dev libxcursor-dev libxi-dev libxrandr-dev \
        patchelf desktop-file-utils squashfs-tools shellcheck \
        pulseaudio pulseaudio-utils xvfb xauth \
    && rm -rf /var/lib/apt/lists/*

RUN apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        pipewire-bin pipewire-pulse wireplumber dbus-daemon \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add clippy rustfmt

WORKDIR /work
