#!/usr/bin/env bash
set -euo pipefail
repository_root="${1:?repository root}"
stage="${2:?staging directory}"
rust_target="${3:?Rust target}"
rust_license_root="$stage/licenses/rust"
rust_dependency_list="$stage/licenses/RUST-DEPENDENCIES.txt"
mkdir -p "$rust_license_root"
: >"$rust_dependency_list"
while read -r package_name package_version _; do
    package_version="${package_version#v}"
    case "$package_name" in
        noise-hoihoi-app | noise-hoihoi-engine | noise-net | noise-net-runtime)
            continue
            ;;
    esac

    package_directory="$(
        find "$CARGO_HOME/registry/src" \
            -mindepth 2 \
            -maxdepth 2 \
            -type d \
            -name "$package_name-$package_version" \
            -print \
            -quit
    )"
    if [[ -z "$package_directory" ]]; then
        echo "Could not locate sources for Rust dependency $package_name $package_version." >&2
        exit 1
    fi

    declared_license="$(
        sed -n 's/^license = "\(.*\)"$/\1/p' "$package_directory/Cargo.toml" | head -n 1
    )"
    printf '%s %s\t%s\n' \
        "$package_name" \
        "$package_version" \
        "${declared_license:-see bundled license files}" \
        >>"$rust_dependency_list"

    package_license_root="$rust_license_root/$package_name-$package_version"
    mkdir -p "$package_license_root"
    license_count=0
    while IFS= read -r -d '' license_file; do
        relative_license="${license_file#"$package_directory"/}"
        mkdir -p "$package_license_root/$(dirname -- "$relative_license")"
        cp -- "$license_file" "$package_license_root/$relative_license"
        license_count=$((license_count + 1))
    done < <(
        find "$package_directory" \
            -maxdepth 3 \
            -type f \
            \( \
                -iname 'COPYING*' -o \
                -iname 'COPYRIGHT*' -o \
                -iname 'FONTLOG*' -o \
                -ipath '*/fonts/*.txt' -o \
                -iname 'LICENSE*' -o \
                -iname 'NOTICE*' -o \
                -iname 'OFL*' -o \
                -iname 'UFL*' -o \
                -iname 'UNLICENSE*' \
            \) \
            -print0 \
            | sort -z
    )
    if ((license_count == 0)); then
        case "$package_name:$declared_license" in
            mutants:MIT)
                cp -- \
                    "$repository_root/packaging/licenses/mutants-LICENSE-MIT.txt" \
                    "$package_license_root/LICENSE-MIT"
                ;;
            pulp-wasm-simd-flag:MIT)
                cp -- \
                    "$repository_root/packaging/licenses/pulp-LICENSE-MIT.txt" \
                    "$package_license_root/LICENSE-MIT"
                ;;
            realfft:MIT)
                cp -- \
                    "$repository_root/packaging/licenses/realfft-LICENSE-MIT.txt" \
                    "$package_license_root/LICENSE-MIT"
                ;;
            *:*Apache-2.0* | *:BSL-1.0 | *:MPL-2.0) ;;
            *)
                echo "Rust dependency $package_name $package_version has no bundled license file or supported common-license fallback." >&2
                exit 1
                ;;
        esac
        cp -- "$package_directory/Cargo.toml" "$package_license_root/Cargo.toml"
        for attribution_file in Cargo.toml.orig README.md; do
            if [[ -f "$package_directory/$attribution_file" ]]; then
                cp -- "$package_directory/$attribution_file" \
                    "$package_license_root/$attribution_file"
            fi
        done
    fi
done < <(
    cargo tree \
        --locked \
        --target "$rust_target" \
        --edges normal,build \
        --prefix none \
        --format '{p}' \
        -p noise-hoihoi-app \
        | sed 's/ (\*)$//' \
        | sort -u
)
