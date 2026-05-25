#!/bin/bash
# Launch Darkroom inside the KasmVNC desktop session.
# Loops so Darkroom restarts automatically if it exits or crashes.

while true; do
  /usr/local/bin/darkroom \
    --configdir "${DARKROOM_CONFIGDIR:-/config/darkroom}" \
    --cachedir  "${DARKROOM_CACHEDIR:-/config/cache}"
  echo "[autostart] Darkroom exited (code $?), restarting in 3s..."
  sleep 3
done
