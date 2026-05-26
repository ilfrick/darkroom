#!/bin/bash
# Runs as root before the KasmVNC session starts (linuxserver s6-overlay cont-init).
# Creates Darkroom config/cache dirs and ensures the container user owns them.

PUID=${PUID:-911}
PGID=${PGID:-911}

mkdir -p /config/darkroom /config/cache
chown -R "${PUID}:${PGID}" /config/darkroom /config/cache
chmod 750 /config/darkroom /config/cache
