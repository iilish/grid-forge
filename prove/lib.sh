#!/usr/bin/env bash
# Helpers for check functions. Sourced by prove.sh; checks run in a subshell with
# `set -euo pipefail`, so any failing command fails the check.
#
# A check passes by returning 0, is PENDING if it returns 77, and fails otherwise.

# Log to stderr (shown in the proof output when a check fails or is pending).
log() { printf '%s\n' "$*" >&2; }

# Mark a check as not implemented yet (PENDING).
pending() { log "pending: ${*:-not implemented}"; return 77; }

# Fail unless every named tool is on PATH.
need() {
  local c
  for c in "$@"; do
    command -v "$c" >/dev/null 2>&1 || { log "missing tool: $c"; return 1; }
  done
}

# Fail unless every named environment variable is set and non-empty.
require_env_vars() {
  local v
  for v in "$@"; do
    [[ -n "${!v:-}" ]] || { log "environment variable $v is not set (see prove/env/${GP_ENV:-local}.sh)"; return 1; }
  done
}

# HTTP status code only: http_status <curl args...>
http_status() { curl -s -o /dev/null -w '%{http_code}' --max-time "${HTTP_TIMEOUT:-10}" "$@"; }

# HTTP body, failing on transport errors: http_body <curl args...>
http_body() { curl -sS --max-time "${HTTP_TIMEOUT:-10}" "$@"; }

# Poll until a command succeeds: wait_until <timeout_s> <interval_s> <cmd...>
wait_until() {
  local timeout="$1" interval="$2"; shift 2
  local end=$((SECONDS + timeout))
  until "$@"; do
    (( SECONDS >= end )) && { log "timed out after ${timeout}s waiting for: $*"; return 1; }
    sleep "$interval"
  done
}

# Assertions: assert_eq <actual> <expected> [label]
assert_eq() {
  [[ "$1" == "$2" ]] || { log "assert_eq failed${3:+ ($3)}: expected '$2', got '$1'"; return 1; }
}
# assert_le <actual> <limit> [label]  (numeric, actual <= limit)
assert_le() {
  awk -v a="$1" -v b="$2" 'BEGIN{exit !(a<=b)}' || { log "assert_le failed${3:+ ($3)}: $1 > $2"; return 1; }
}
# assert_ge <actual> <limit> [label]  (numeric, actual >= limit)
assert_ge() {
  awk -v a="$1" -v b="$2" 'BEGIN{exit !(a>=b)}' || { log "assert_ge failed${3:+ ($3)}: $1 < $2"; return 1; }
}

# Milliseconds since epoch (for timing a flow end to end).
now_ms() { date +%s%3N; }

# Record a measured number for the blog and the DORA chart: record_metric <name> <value>
# Appends to prove/results/metrics.tsv: timestamp, env, name, value.
record_metric() {
  local dir="${PROVE_RESULTS_DIR:-.}"
  mkdir -p "$dir"
  printf '%s\t%s\t%s\t%s\n' "$(date -u +%FT%TZ)" "${GP_ENV:-local}" "$1" "$2" >>"$dir/metrics.tsv"
}
