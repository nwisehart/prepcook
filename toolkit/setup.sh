#!/usr/bin/env bash
# Installs the prepcook-* skills into Claude Code on this machine and registers
# the prepcook MCP that reaches the knowledge store.
#
#   toolkit/setup.sh --backend-url URL [--mcp PATH]   install or refresh
#   toolkit/setup.sh --no-mcp                         skills only, no knowledge store
#   toolkit/setup.sh --uninstall                      remove what this script put in place
#
# --backend-url (or PREPCOOK_BACKEND_URL) is the Prep Cook server whose knowledge store
# the RCAs are filed in. --mcp is the prepcook-mcp binary; without it, the one on
# PATH. Build it with `cargo build --release --bin prepcook-mcp` (it lands in
# target/release/) or take it from a GitHub release of Prep Cook.
#
# Idempotent: run it again after a `git pull`. It never touches your app, its
# manifest or anything the seed wrote.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SKILLS_DIR="$HOME/.claude/skills"
MCP_NAME="prepcook"

say()  { printf '%s\n' "$*"; }
step() { printf '\n== %s\n' "$*"; }
die()  { printf 'setup.sh: %s\n' "$*" >&2; exit 1; }

usage() {
  # The comment block at the top of this file.
  awk 'NR > 1 && /^#/ { sub(/^# ?/, ""); print; next } NR > 1 { exit }' "${BASH_SOURCE[0]}"
}

ours() {
  # A symlink under ~/.claude/skills that points into this checkout is ours to replace.
  [ -L "$1" ] && case "$(readlink "$1")" in "$HERE"/*) return 0;; esac
  return 1
}

uninstall() {
  step "Removing skill links from $SKILLS_DIR"
  local removed=0
  for path in "$SKILLS_DIR"/*; do
    [ -e "$path" ] || [ -L "$path" ] || continue
    if ours "$path"; then rm "$path"; say "removed $path"; removed=1; fi
  done
  [ "$removed" = 1 ] || say "none of ours"
  step "Removing the $MCP_NAME MCP entry"
  if command -v claude >/dev/null 2>&1 && claude mcp remove -s user "$MCP_NAME" >/dev/null 2>&1; then
    say "removed"
  else
    say "none registered"
  fi
}

backend_url="${PREPCOOK_BACKEND_URL:-}"
mcp_bin=""
register_mcp=1
while [ $# -gt 0 ]; do
  case "$1" in
    --uninstall) uninstall; exit 0 ;;
    --backend-url) [ $# -ge 2 ] || die "--backend-url needs a URL"; backend_url="$2"; shift 2 ;;
    --backend-url=*) backend_url="${1#*=}"; shift ;;
    --mcp) [ $# -ge 2 ] || die "--mcp needs a path"; mcp_bin="$2"; shift 2 ;;
    --mcp=*) mcp_bin="${1#*=}"; shift ;;
    --no-mcp) register_mcp=0; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
done

step "Checking tools"
missing=()
for tool in claude node; do
  if command -v "$tool" >/dev/null 2>&1; then say "$tool: $(command -v "$tool")"; else missing+=("$tool"); fi
done
host_cli=""
for tool in gh glab; do
  if command -v "$tool" >/dev/null 2>&1; then say "$tool: $(command -v "$tool")"; host_cli="$tool"; fi
done
[ -n "$host_cli" ] || missing+=("gh or glab")
if [ ${#missing[@]} -gt 0 ]; then
  die "not on PATH: ${missing[*]}. claude runs the skills, node runs the harness, and gh or glab (whichever your host uses) opens the pull request. Install them and run this again."
fi

if [ "$register_mcp" = 1 ]; then
  # Resolve everything before changing anything, so a bad argument leaves the
  # machine as it was.
  [ -n "$backend_url" ] || die "no Prep Cook server named: pass --backend-url URL (or set PREPCOOK_BACKEND_URL) for the server whose knowledge store RCAs are filed in, or --no-mcp to install the skills alone"
  if [ -z "$mcp_bin" ]; then
    mcp_bin="$(command -v prepcook-mcp || true)"
    [ -n "$mcp_bin" ] || die "prepcook-mcp is not on PATH: build it with \`cargo build --release --bin prepcook-mcp\` in a Prep Cook checkout (or take it from a GitHub release of Prep Cook) and pass --mcp PATH"
  fi
  [ -x "$mcp_bin" ] || die "$mcp_bin is not an executable file"
  mcp_bin="$(cd "$(dirname "$mcp_bin")" && pwd)/$(basename "$mcp_bin")"
fi

step "Linking skills into $SKILLS_DIR"
mkdir -p "$SKILLS_DIR"
for skill in "$HERE"/skills/*/; do
  name="$(basename "$skill")"
  target="$SKILLS_DIR/$name"
  if [ -L "$target" ]; then
    if ours "$target"; then
      ln -sfn "${skill%/}" "$target"; say "$name: link refreshed"
    else
      say "$name: SKIPPED — $target is a symlink to $(readlink "$target"), not ours; remove it if you want this one"
    fi
  elif [ -e "$target" ]; then
    say "$name: SKIPPED — $target is a real directory; move it aside if you want this one"
  else
    ln -s "${skill%/}" "$target"; say "$name: linked"
  fi
done

if [ "$register_mcp" = 1 ]; then
  step "Registering the $MCP_NAME MCP with Claude Code"
  claude mcp remove -s user "$MCP_NAME" >/dev/null 2>&1 || true
  claude mcp add -s user "$MCP_NAME" -e "PREPCOOK_BACKEND_URL=$backend_url" -- "$mcp_bin" --mode global >/dev/null
  say "$MCP_NAME → $mcp_bin --mode global (PREPCOOK_BACKEND_URL=$backend_url)"
else
  step "Skipping the $MCP_NAME MCP (--no-mcp)"
  say "/prepcook-rca cannot file RCAs in the knowledge store, or read past ones, until you run this again with --backend-url"
fi

step "Next"
if claude mcp list 2>/dev/null | grep -qi -e atlassian -e jira -e linear -e tracker; then
  say "- a tracker MCP looks connected"
else
  say "- connect your tracker's MCP once (for Jira, Atlassian's MCP server, per Atlassian's instructions); the skills read and write the ticket through it"
fi
say "- log $host_cli in to your host if it is not already ($host_cli auth status)"
say "- stand your app up from its .prepcook/scripts.toml and export the seed's variables (toolkit/README.md, \"Standing the app up\")"
say "- then, from that checkout: /prepcook-repro <KEY>"
