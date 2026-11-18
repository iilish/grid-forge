# Implementing checks

Each registered ID in `../registry.tsv` is implemented as a bash function named `check_<ID with dashes replaced by underscores>`, for example `check_P0_A1` for `P0-A1`. Put them in `p0.sh`, `p1.sh` and so on; the runner sources every `*.sh` file in this directory.

- Return 0 to pass, 77 (use `pending "reason"`) when it cannot be checked yet, anything else to fail.
- Checks run in a subshell with `set -euo pipefail`, so a failing command fails the check.
- Helpers (`need`, `wait_until`, `http_status`, `assert_eq`, `record_metric`, ...) come from `../lib.sh`.
- Settings such as `BASE_URL` come from `../env/<env>.sh`.
- A registered ID with no function reports PENDING. `--allow-pending` tolerates that while you build; CI and staging do not.

Write the check before the feature, watch it fail, then build. Example of the shape (adapt the routes to your app; they are illustrative, not part of the spec):

```bash
# prove/checks/p0.sh
check_P0_A3() {   # /readyz returns non-200 when the database is unreachable
  need curl kubectl
  require_env_vars KUBE_CONTEXT
  kubectl --context "$KUBE_CONTEXT" -n GridForge scale statefulset/postgres --replicas=0
  trap 'kubectl --context "$KUBE_CONTEXT" -n GridForge scale statefulset/postgres --replicas=1' EXIT
  wait_until 60 2 bash -c "[[ \$(curl -s -o /dev/null -w '%{http_code}' \"$BASE_URL/readyz\") != 200 ]]"
}
```

Rules of thumb:

- A drill must assert the fault actually took effect before it asserts recovery (see the Phase 12 silent-failure register).
- Record numbers you will want later with `record_metric`, for example `record_metric lead_time_s "$secs"` or `record_metric rollback_s "$secs"`.
- Do not weaken a check to make it pass. If the check is wrong, change it deliberately in its own commit and say why.
