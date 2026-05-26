#!/bin/bash
# Runs as root after linuxserver's own user-setup cont-init (prefix 50-).
# By this point 'abc' has been remapped to PUID:PGID by the base image.

CONT_UID=$(id -u abc 2>/dev/null || echo "${PUID:-1000}")
CONT_GID=$(id -g abc 2>/dev/null || echo "${PGID:-1000}")

# Ensure /config itself is owned and writable by the container user.
# Docker auto-creates bind-mount directories as root:root mode 755, which
# lets abc READ /config but not CREATE new entries inside it.
chown "${CONT_UID}:${CONT_GID}" /config
chmod 755 /config

# Pre-create the darkroom-specific subdirectories
mkdir -p /config/darkroom /config/cache
chown -R "${CONT_UID}:${CONT_GID}" /config/darkroom /config/cache
chmod 750 /config/darkroom /config/cache
