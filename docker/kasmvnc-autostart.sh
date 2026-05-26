#!/bin/bash
# Launch Darkroom inside the KasmVNC desktop session.
# Loops so Darkroom restarts automatically if it exits or crashes.

# Belt-and-suspenders: ensure config/cache dirs exist as the desktop user.
# The cont-init script (50-darkroom-dirs) runs as root earlier, but if the
# bind-mount timing or permissions caused it to fail, this catches it.
mkdir -p "${DARKROOM_CONFIGDIR:-/config/darkroom}" \
         "${DARKROOM_CACHEDIR:-/config/cache}" 2>/dev/null

while true; do
  /usr/local/bin/darkroom \
    --configdir "${DARKROOM_CONFIGDIR:-/config/darkroom}" \
    --cachedir  "${DARKROOM_CACHEDIR:-/config/cache}"
  echo "[autostart] Darkroom exited (code $?), restarting in 3s..."
  sleep 3
done
