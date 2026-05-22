#!/bin/bash
# Launch Darkroom inside the KasmVNC desktop session
exec /usr/local/bin/darkroom \
    --configdir "${DARKROOM_CONFIGDIR:-/config/darkroom}" \
    --cachedir  "${DARKROOM_CACHEDIR:-/config/cache}" \
    "${@}"
