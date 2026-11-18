# GridForge proofs

The proof harness turns the project's definition of done into runnable checks. It is a small Bash runner, not a general test framework.

A **proof** is one check with a stable ID:

- `P0-A1` — acceptance check.
- `P0-S1` — silent-failure detection.
- `P0-R1` — rollback drill.

The number after `P` is a **delivery gate**. Gate 0 is the walking skeleton; later gates add capability. The runner calls gates "phases" in its output, but a gate is simply a delivery milestone with its own deployable outcome.

## Commands

```bash
just prove 0 local                 # gates 0..0 against the local stack
just prove 3 staging               # gates 0..3 against staging
just prove 3 local --allow-pending # during development only
just prove 0 local --only P0-A1    # one check
just prove-list                    # every registered check
just prove-lint                    # registry and implementation consistency
```

`just prove <gate> <env>` always runs every registered check up to and including `<gate>`. A later gate cannot silently break an earlier one.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Every selected check passed. |
| `1` | At least one check failed. |
| `2` | No failures, but at least one check is `PENDING`. |

`PENDING` fails in CI and on staging. `--allow-pending` exists only for local development and must never be used in a deploy gate.

A registered check with no implementation reports `PENDING`. This is deliberate: the check can be written and watched failing before the feature exists.

## Registry

`prove/registry.tsv` is the source of truth for public checks. Each row has:

```
id	gate	kind	description
P0-A1	0	acceptance	Clicking 'send test event' ... within 5 seconds
```

The check implementation is a Bash function named after the ID:

```bash
check_P0_A1() {
  # assert the observable outcome
}
```

Implementations live in `prove/checks/p<gate>.sh`. They source `prove/lib.sh`, which provides HTTP, wait, assertion and metric helpers.

## Adding a proof

1. Add the row to `prove/registry.tsv`.
2. Add `check_P<gate>_<kind><n>` to `prove/checks/p<gate>.sh`.
3. Run `just prove <gate> local --only <ID>` and watch it fail for the right reason.
4. Implement the feature until it passes.
5. Run `just prove-lint`.

In a checkout with the planning documents, the registry, the check implementations and every documented ID must agree. In a public checkout without those documents, `just prove-lint` checks the registry and implementations and skips the documentation comparison.

## Results

Every run writes a TSV under `prove/results/`:

```text
prove/results/20261118T120000Z-local-p0.tsv
```

The file contains one row per selected check with `id`, `status` and `description`. Checks can also record metrics with `record_metric`; those results feed the project's lead-time, rollback-time and time-to-ack reports.

`prove/results/` is gitignored.

## Environments

Environment settings live in `prove/env/<env>.sh`. The default environment is `local`.

- `BASE_URL` — the HTTP endpoint under test.
- `KUBE_NAMESPACE`, `PROD_NAMESPACE` — cluster namespaces for deploy gates.
- `GH_REPO` — the repository used for CI and lead-time checks.

Local checks use `BASE_URL=http://localhost:8080` unless overridden.

## Writing a good proof

- Assert the user-visible outcome, not an internal implementation detail.
- Prefer an HTTP request, a database query or a Kubernetes read over grepping logs.
- Make the failure message say what was expected, what happened and where to look.
- Never weaken a check to make a run pass. If the check is wrong, change it explicitly and explain why.
- A silent-failure check must prove that the failure is detected, not merely that the happy path works.
