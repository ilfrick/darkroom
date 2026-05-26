#!/bin/bash
# Runs as root after linuxserver's own user-setup cont-init (hence prefix 50-).
# By this point the base image has already remapped 'abc' to PUID:PGID,
# so we use 'id -u abc' / 'id -g abc' to get the real mapped uid/gid
# rather than reading PUID/PGID directly (which may not be set in all
# run configurations).

CONT_UID=$(id -u abc 2>/dev/null || echo "${PUID:-1000}")
CONT_GID=$(id -g abc 2>/dev/null || echo "${PGID:-1000}")

mkdir -p /config/darkroom /config/cache
chown -R "${CONT_UID}:${CONT_GID}" /config/darkroom /config/cache
chmod 750 /config/darkroom /config/cache
