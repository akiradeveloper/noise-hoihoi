#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd -- "$script_dir/../.." && pwd)"
image="${NOISE_HOIHOI_DOCKER_IMAGE:-noise-hoihoi-windows-dev:local}"
cargo_cache_volume="${NOISE_HOIHOI_DOCKER_CARGO_CACHE:-noise-hoihoi-cargo-cache}"

if ! command -v docker >/dev/null 2>&1; then
    echo "Docker was not found." >&2
    exit 1
fi

source_date_epoch="${SOURCE_DATE_EPOCH:-}"
if [[ -z "$source_date_epoch" ]]; then
    if ! source_date_epoch="$(git -C "$repository_root" log -1 --format=%ct 2>/dev/null)"; then
        echo "Set SOURCE_DATE_EPOCH when building outside a Git checkout." >&2
        exit 1
    fi
fi
if [[ ! "$source_date_epoch" =~ ^[0-9]+$ ]]; then
    echo "SOURCE_DATE_EPOCH must be a non-negative integer." >&2
    exit 1
fi

docker build \
    --tag "$image" \
    --file "$repository_root/packaging/windows/docker/Dockerfile" \
    "$repository_root/packaging/windows/docker"

docker volume create "$cargo_cache_volume" >/dev/null
docker_arguments=(
    run --rm
    --volume "$repository_root:/work"
    --volume "$cargo_cache_volume:/root/.cargo-cache"
    --workdir /work
    --env CARGO_HOME=/root/.cargo-cache
    --env "HOST_UID=$(id -u)"
    --env "HOST_GID=$(id -g)"
    --env "NOISE_HOIHOI_JOBS=${NOISE_HOIHOI_JOBS:-}"
    --env "SOURCE_DATE_EPOCH=$source_date_epoch"
)
docker_arguments+=("$image" bash /work/scripts/docker/build-windows-inner.sh)

docker "${docker_arguments[@]}"
