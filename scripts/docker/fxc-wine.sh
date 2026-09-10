#!/usr/bin/env bash
set -euo pipefail
export WINEDEBUG=-all
export WINEPREFIX=/tmp/noise-hoihoi-wine
export WINEDLLOVERRIDES=d3dcompiler_47=n
arguments=()
for argument in "$@"; do
    case "$argument" in
        /work/* | /root/*) argument="Z:${argument//\//\\}" ;;
    esac
    arguments+=("$argument")
done
exec /usr/lib/wine/wine64 /work/target/gpui-sdk/bin/fxc.exe "${arguments[@]}"
