#!/bin/bash
# Launch Darkroom inside the KasmVNC desktop session.
# Loops so Darkroom restarts automatically if it exits or crashes.
#
# Signal forwarding: `docker stop` (and s6 service shutdown) delivers
# SIGTERM to the openbox session, not to this script's children, so by
# default darkroom is reparented to PID 1 and eventually SIGKILL'd —
# its SIGTERM handler never fires, dt_cleanup() never runs, and the
# user's preferences are never flushed to darktablerc. We install a
# trap that forwards SIGTERM/SIGINT/SIGHUP to the running darkroom
# child and waits for it to exit cleanly so dt_conf_save() can run.

# Belt-and-suspenders: ensure config/cache dirs exist as the desktop user.
# The cont-init script (50-darkroom-dirs) runs as root earlier, but if the
# bind-mount timing or permissions caused it to fail, this catches it.
mkdir -p "${DARKROOM_CONFIGDIR:-/config/darkroom}" \
         "${DARKROOM_CACHEDIR:-/config/cache}" 2>/dev/null

# These are set by the loop and inspected by the trap.
child_pid=""
shutdown_requested=0

_forward_signal() {
  shutdown_requested=1
  if [ -n "$child_pid" ] && kill -0 "$child_pid" 2>/dev/null; then
    echo "[autostart] forwarding $1 to darkroom (pid $child_pid)"
    kill -"$1" "$child_pid" 2>/dev/null
    # Wait up to ~15s for darkroom to flush conf and exit on its own.
    # dt_cleanup() can take a few seconds (pipeline teardown, image cache
    # write-back, db close), so don't escalate too quickly.
    for _ in $(seq 1 30); do
      kill -0 "$child_pid" 2>/dev/null || break
      sleep 0.5
    done
  fi
  exit 0
}

trap '_forward_signal TERM' TERM
trap '_forward_signal INT'  INT
trap '_forward_signal HUP'  HUP

while true; do
  /usr/local/bin/darkroom \
    --configdir "${DARKROOM_CONFIGDIR:-/config/darkroom}" \
    --cachedir  "${DARKROOM_CACHEDIR:-/config/cache}" &
  child_pid=$!
  # `wait` is interruptible by signals, so the trap above can run while we
  # block here. If the signal came in, the trap calls exit() so the loop
  # never iterates again.
  wait "$child_pid"
  rc=$?
  child_pid=""
  if [ "$shutdown_requested" = "1" ]; then
    exit 0
  fi
  echo "[autostart] Darkroom exited (code $rc), restarting in 3s..."
  sleep 3
done
