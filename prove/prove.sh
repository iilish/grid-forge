#!/usr/bin/env bash
# GridForge proof runner.
#
#   prove/prove.sh <phase 0..12> [env] [--allow-pending] [--only ID]
#   prove/prove.sh --list
#   prove/prove.sh --lint
#
# Runs every registered check for phases 0..<phase> against <env> (default: local).
# Check IDs look like P3-A2 (acceptance), P3-S1 (silent failure), P3-R1 (rollback drill).
# A check is a bash function check_P3_A2 defined in prove/checks/*.sh.
#
# Exit codes: 0 all pass | 1 at least one FAIL | 2 no FAIL but some PENDING
#             (PENDING is tolerated only with --allow-pending)
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$ROOT/.." && pwd)"
REGISTRY="${PROVE_REGISTRY:-$ROOT/registry.tsv}"
CHECKS_DIR="${PROVE_CHECKS_DIR:-$ROOT/checks}"
DOCS_DIR="${PROVE_DOCS_DIR:-$REPO/docs/phases}"
RESULTS_DIR="${PROVE_RESULTS_DIR:-$ROOT/results}"
export PROVE_RESULTS_DIR="$RESULTS_DIR"

# shellcheck source=lib.sh
source "$ROOT/lib.sh"

usage() {
  cat <<'EOF'
usage: prove/prove.sh <phase 0..12> [env] [--allow-pending] [--only ID]
       prove/prove.sh --list
       prove/prove.sh --lint
EOF
}

registry_rows() { grep -vE '^\s*(#|$)' "$REGISTRY"; }

list_checks() {
  printf '%-8s %-6s %-11s %s\n' ID PHASE KIND DESCRIPTION
  registry_rows | while IFS=$'\t' read -r id phase kind desc; do
    printf '%-8s %-6s %-11s %s\n' "$id" "$phase" "$kind" "$desc"
  done
}

# lint: docs, registry and implementations must agree.
lint() {
  local rc=0 docs_ids reg_ids impl_ids out
  docs_ids=$(grep -rhoE 'P[0-9]+-[ASR][0-9]+' "$DOCS_DIR" 2>/dev/null | sort -u)
  reg_ids=$(registry_rows | cut -f1 | sort -u)
  impl_ids=$(grep -rhoE --include='*.sh' '^check_P[0-9]+_[ASR][0-9]+' "$CHECKS_DIR" 2>/dev/null \
             | sed -E 's/^check_//; s/_/-/' | sort -u)

  out=$(comm -23 <(printf '%s\n' "$docs_ids") <(printf '%s\n' "$reg_ids") | sed '/^$/d')
  [[ -n "$out" ]] && { echo "In docs but not in registry:"; echo "$out" | sed 's/^/  /'; rc=1; }

  out=$(comm -13 <(printf '%s\n' "$docs_ids") <(printf '%s\n' "$reg_ids") | sed '/^$/d')
  [[ -n "$out" ]] && { echo "In registry but not in docs:"; echo "$out" | sed 's/^/  /'; rc=1; }

  out=$(comm -13 <(printf '%s\n' "$reg_ids") <(printf '%s\n' "$impl_ids") | sed '/^$/d')
  [[ -n "$out" ]] && { echo "Implemented but not registered:"; echo "$out" | sed 's/^/  /'; rc=1; }

  out=$(registry_rows | cut -f1 | sort | uniq -d)
  [[ -n "$out" ]] && { echo "Duplicate IDs in registry:"; echo "$out" | sed 's/^/  /'; rc=1; }

  if (( rc == 0 )); then
    echo "lint ok: $(printf '%s\n' "$reg_ids" | sed '/^$/d' | wc -l | tr -d ' ') checks registered, $(printf '%s\n' "$impl_ids" | sed '/^$/d' | wc -l | tr -d ' ') implemented"
  fi
  return "$rc"
}

load_env() {
  local env="$1"
  export GP_ENV="$env"
  if [[ -f "$ROOT/env/$env.sh" ]]; then
    # shellcheck disable=SC1090
    source "$ROOT/env/$env.sh"
  elif [[ "$env" != "local" ]]; then
    echo "warning: prove/env/$env.sh not found; checks needing BASE_URL etc. will fail" >&2
  fi
  : "${BASE_URL:=http://localhost:8080}"
  export BASE_URL
}

run_checks() {
  local max="$1" env="$2" only="$3" allow_pending="$4"
  local pass=0 fail=0 pend=0 last_phase="" logf rc status fn f
  local stamp out_file

  load_env "$env"

  if [[ -d "$CHECKS_DIR" ]]; then
    for f in "$CHECKS_DIR"/*.sh; do
      [[ -e "$f" ]] || continue
      # shellcheck disable=SC1090
      source "$f"
    done
  fi

  mkdir -p "$RESULTS_DIR"
  stamp="$(date -u +%Y%m%dT%H%M%SZ)"
  out_file="$RESULTS_DIR/${stamp}-${env}-p${max}.tsv"
  printf 'id\tstatus\tdescription\n' >"$out_file"

  echo "GridForge proofs: phases 0..$max against '$env' (BASE_URL=$BASE_URL)"

  while IFS=$'\t' read -r id phase kind desc; do
    (( phase > max )) && continue
    [[ -n "$only" && "$id" != "$only" ]] && continue
    if [[ "$phase" != "$last_phase" ]]; then
      printf '\nPhase %s\n' "$phase"
      last_phase="$phase"
    fi
    fn="check_${id//-/_}"
    if declare -F "$fn" >/dev/null; then
      logf="$(mktemp)"
      ( set -euo pipefail; "$fn" ) >"$logf" 2>&1
      rc=$?
      case "$rc" in
        0)  status=PASS;    pass=$((pass + 1)) ;;
        77) status=PENDING; pend=$((pend + 1)) ;;
        *)  status=FAIL;    fail=$((fail + 1)) ;;
      esac
    else
      logf=""
      status=PENDING
      pend=$((pend + 1))
    fi
    printf '  %-8s %-8s %s\n' "$status" "$id" "$desc"
    printf '%s\t%s\t%s\n' "$id" "$status" "$desc" >>"$out_file"
    if [[ -n "$logf" ]]; then
      if [[ "$status" == FAIL || "$status" == PENDING ]]; then
        tail -n 20 "$logf" | sed 's/^/             | /'
      fi
      rm -f "$logf"
    fi
  done < <(registry_rows)

  printf '\nSummary: %d pass, %d fail, %d pending  (results: %s)\n' \
    "$pass" "$fail" "$pend" "${out_file#"$REPO"/}"

  (( fail > 0 )) && return 1
  if (( pend > 0 )) && [[ "$allow_pending" != 1 ]]; then
    echo "PENDING checks are not allowed without --allow-pending" >&2
    return 2
  fi
  return 0
}

main() {
  local phase="" env="" only="" allow_pending=0 mode=run
  while (( $# )); do
    case "$1" in
      --list) mode=list ;;
      --lint) mode=lint ;;
      --allow-pending) allow_pending=1 ;;
      --only) shift; only="${1:-}" ;;
      -h|--help) usage; return 0 ;;
      -*) echo "unknown option: $1" >&2; usage >&2; return 64 ;;
      *) if [[ -z "$phase" ]]; then phase="$1"; elif [[ -z "$env" ]]; then env="$1"; else
           echo "unexpected argument: $1" >&2; usage >&2; return 64; fi ;;
    esac
    shift
  done

  [[ -f "$REGISTRY" ]] || { echo "registry not found: $REGISTRY" >&2; return 66; }

  case "$mode" in
    list) list_checks; return 0 ;;
    lint) lint; return $? ;;
  esac

  [[ "$phase" =~ ^[0-9]+$ ]] || { usage >&2; return 64; }
  run_checks "$phase" "${env:-local}" "$only" "$allow_pending"
}

main "$@"
