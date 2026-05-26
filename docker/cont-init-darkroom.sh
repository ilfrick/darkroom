#!/bin/bash
# s6 cont-init — runs as root before services start.
# Creates /config/darkroom and /config/cache with the ownership that
# matches the runtime user (PUID:PGID, set in docker-compose.yml or
# via -e PUID=... on docker run).  Defaults to 1000:1000.
#
# We use ${PUID} directly (not 'id -u abc') because linuxserver's own
# 10-adduser script may not have run yet when this script executes.

CONT_UID=${PUID:-1000}
CONT_GID=${PGID:-1000}

# /config is the bind-mount root.  Docker auto-creates it as root:root
# mode 755, which lets the container user traverse it but NOT write to
# it.  Chown it to the runtime user so new entries can be created.
chown "${CONT_UID}:${CONT_GID}" /config 2>/dev/null || true
chmod 755 /config

# Pre-create darkroom-specific subdirectories.
mkdir -p /config/darkroom /config/cache
chown -R "${CONT_UID}:${CONT_GID}" /config/darkroom /config/cache
chmod 750 /config/darkroom /config/cache

echo "[darkroom-init] /config owned by $(stat -c '%u:%g' /config), darkroom by $(stat -c '%u:%g' /config/darkroom)"
