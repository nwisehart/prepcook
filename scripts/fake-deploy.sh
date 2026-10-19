#!/usr/bin/env bash
# A stand-in for a preview-environment deploy, for exercising the `remote`
# reproduction target on one machine (docs/manual-testing.md, section 6).
#
# Starts Pantry (the prepcook-demo app) from the worktree in the background on
# a free port, waits for /health, and reports the deploy the way
# [app.remote] start must (docs/scripts.md#the-script-contract):
# PREPCOOK_APP_URL and PREPCOOK_TARGET_REF appended to $PREPCOOK_ENV_OUT, and
# the URL as the last line of stdout. scripts/fake-destroy.sh stops it by the
# ref. Each deploy leaves <ref>.pid and <ref>.log in
# ${PREPCOOK_FAKE_DEPLOY_DIR:-${TMPDIR:-/tmp}/prepcook-fake-deploy}.
#
# Usage: fake-deploy.sh [app dir]   (default $PREPCOOK_WORKTREE, else the
#                                    current directory)
set -euo pipefail

: "${PREPCOOK_ENV_OUT:?PREPCOOK_ENV_OUT must name the file the deploy is reported in}"
app_dir="${1:-${PREPCOOK_WORKTREE:-$PWD}}"
state_dir="${PREPCOOK_FAKE_DEPLOY_DIR:-${TMPDIR:-/tmp}/prepcook-fake-deploy}"

if [ ! -f "$app_dir/server.js" ]; then
  echo "fake-deploy: no server.js in $app_dir; is it a Pantry checkout?" >&2
  exit 1
fi

# The run id is unique per stand-up; the ref only keeps letters and digits so
# it is safe as a file name.
run_id="${PREPCOOK_RUN_ID:-manual-$$}"
ref="fake-$(printf %s "$run_id" | tr -cd 'A-Za-z0-9' | cut -c1-12)"

# Asks the OS for a free port. Another process could take it before Pantry
# binds it; a failed bind shows up below as Pantry exiting.
port="$(node -e 'const s = require("net").createServer();
s.listen(0, "127.0.0.1", () => { console.log(s.address().port); s.close(); });')"
url="http://127.0.0.1:$port"

mkdir -p "$state_dir"
log="$state_dir/$ref.log"
# Detached from this script's stdin and stdout, so the deploy step can exit
# while Pantry keeps serving. `$!` is node's own pid (nohup execs it), which
# is what fake-destroy.sh signals.
(
  cd "$app_dir"
  PORT="$port" nohup node server.js >"$log" 2>&1 </dev/null &
  echo $! >"$state_dir/$ref.pid"
)
pid="$(cat "$state_dir/$ref.pid")"

for _ in $(seq 1 60); do
  if curl -fsS -o /dev/null "$url/health" 2>/dev/null; then
    printf 'PREPCOOK_APP_URL=%s\nPREPCOOK_TARGET_REF=%s\n' "$url" "$ref" >>"$PREPCOOK_ENV_OUT"
    echo "fake-deploy: deployed $ref (pid $pid) from $app_dir"
    echo "$url"
    exit 0
  fi
  if ! kill -0 "$pid" 2>/dev/null; then
    echo "fake-deploy: Pantry exited before answering /health; its log:" >&2
    tail -n 20 "$log" >&2
    rm -f "$state_dir/$ref.pid"
    exit 1
  fi
  sleep 0.5
done

echo "fake-deploy: $url/health did not answer within 30 s; stopping pid $pid" >&2
kill "$pid" 2>/dev/null || true
rm -f "$state_dir/$ref.pid"
exit 1
