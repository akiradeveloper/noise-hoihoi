#!/usr/bin/env bash
set -euo pipefail
script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repository_root="$(cd -- "$script_dir/../.." && pwd)"
image="${NOISE_HOIHOI_LINUX_DOCKER_IMAGE:-noise-hoihoi-linux-dev:local}"
cargo_cache_volume="${NOISE_HOIHOI_DOCKER_CARGO_CACHE:-noise-hoihoi-cargo-cache}"
command -v docker >/dev/null || { echo "Docker was not found." >&2; exit 1; }
source_date_epoch="${SOURCE_DATE_EPOCH:-$(git -C "$repository_root" log -1 --format=%ct)}"
[[ "$source_date_epoch" =~ ^[0-9]+$ ]] || { echo "SOURCE_DATE_EPOCH must be an integer." >&2; exit 1; }
docker build --tag "$image" --file "$repository_root/packaging/linux/docker/Dockerfile" "$repository_root/packaging/linux/docker"
docker volume create "$cargo_cache_volume" >/dev/null
docker run --rm \
    --volume "$repository_root:/work" \
    --volume "$cargo_cache_volume:/root/.cargo-cache" \
    --env CARGO_HOME=/root/.cargo-cache \
    --env "HOST_UID=$(id -u)" --env "HOST_GID=$(id -g)" \
    --env "NOISE_HOIHOI_JOBS=${NOISE_HOIHOI_JOBS:-6}" \
    --env "SOURCE_DATE_EPOCH=$source_date_epoch" \
    "$image" bash /work/scripts/docker/build-linux-inner.sh
