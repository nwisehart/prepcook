#!/usr/bin/env bash
# Destroys what scripts/fake-deploy.sh deployed (docs/manual-testing.md,
# section 6): stops the Pantry process recorded for the ref and removes its
# pid file. The ref is $PREPCOOK_TARGET_REF, which Prepcook hands every
# [app.remote] stop, or the first argument.
#
# A missing ref or an unknown one is not an error: teardown runs stop even
# after a deploy that failed, and there is then nothing to destroy.
#
# Usage: fake-destroy.sh [ref]
set -euo pipefail

state_dir="${PREPCOOK_FAKE_DEPLOY_DIR:-${TMPDIR:-/tmp}/prepcook-fake-deploy}"
ref="${1:-${PREPCOOK_TARGET_REF:-}}"

if [ -z "$ref" ]; then
  echo "fake-destroy: no PREPCOOK_TARGET_REF; nothing to destroy"
  exit 0
fi
case "$ref" in
  *[!A-Za-z0-9-]*)
    echo "fake-destroy: '$ref' is not a ref fake-deploy.sh writes" >&2
    exit 2
    ;;
esac

pid_file="$state_dir/$ref.pid"
if [ ! -f "$pid_file" ]; then
  echo "fake-destroy: $ref is not deployed (no $pid_file)"
  exit 0
fi
pid="$(cat "$pid_file")"

# Only a process still running Pantry's server.js is stopped, so a pid the OS
# has since handed to something else is left alone.
if kill -0 "$pid" 2>/dev/null && ps -p "$pid" -o command= | grep -q 'server\.js'; then
  kill "$pid"
  for _ in $(seq 1 20); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 0.25
  done
  if kill -0 "$pid" 2>/dev/null; then
    kill -9 "$pid"
  fi
  echo "fake-destroy: stopped $ref (pid $pid)"
else
  echo "fake-destroy: $ref (pid $pid) was not running"
fi
rm -f "$pid_file"
