# GridForge proofs

`gridforge-prove` is the dev-only Rust proof runner. It turns the project's
definition of done into runnable checks with stable IDs. It is not a general
test framework: no async runtime, no network server, no test harness.

A **proof** is one check with a stable ID:

- `P0-A1` — acceptance check.
- `P0-S1` — silent-failure detection.
- `P0-R1` — rollback drill.

The number after `P` is a **delivery gate**. The runner calls gates "phases" in
its output, but a gate is simply a delivery milestone with its own deployable
outcome.

## Commands

```bash
just prove 0 local                 # gates 0..0 against the local stack
just prove 3 staging               # gates 0..3 against staging
just prove 3 local --allow-pending # during development only
just prove 0 local --only P0-A1    # one check
just prove 0 local --only P0-A1,P0-S3   # comma-separated
just prove 0 local --only P0-A1 --only P0-S3  # repeatable
just prove-list                    # every registered check
just prove-lint                    # registry, docs and implementations agree
```

The underlying binary is:

```text
cargo run -p gridforge-prove --quiet -- <phase 0..12> [env=local] [--allow-pending] [--only ID[,ID...]]
cargo run -p gridforge-prove --quiet -- --list
cargo run -p gridforge-prove --quiet -- --lint
```

`--only` selects proof IDs, never function names. It accepts a
comma-separated list and may be repeated. Repeating an ID is allowed and keeps
one entry. An empty value, such as `--only ,,` or whitespace only, is a usage
error; a trailing comma is accepted. Every selected ID must be registered and
at or below the requested phase, otherwise the run exits `64` before loading
the environment.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Every selected check passed. |
| `1` | At least one check failed, or lint found an issue. |
| `2` | No failures, but at least one check is `PENDING`. |
| `64` | Usage error, including an unknown `--only` ID or one above the requested phase. |

`PENDING` fails in CI and on staging. `--allow-pending` exists only for local
development and must never be used in a deploy gate.

A registered check with no implementation reports `PENDING`. This is
deliberate: the check can be written and watched failing before the feature
exists.

## Check model and naming

Implementations live in `crates/prove/src/checks/p<gate>.rs`. Name the
function after what the check proves, not after its ID:

```rust
fn p0_send_test_event_appears_in_dashboard(ctx: &Ctx) -> Result<CheckOutcome, CheckError> {
    // assert the observable outcome
    Ok(CheckOutcome::Pass)
}
```

Register it with the `check!` macro so the ID and function name cannot drift
apart:

```rust
checks.push(check!("P0-A1", p0_send_test_event_appears_in_dashboard));
```

The future convention is `pN_<what_the_check_proves>`, for example
`p3_delayed_event_is_placed_by_device_time`. Do not use `check_P0_A1`-style
names.

`CheckFn` is `fn(&Ctx) -> Result<CheckOutcome, CheckError>`:

- `CheckOutcome::Pass` — the check passed.
- `CheckOutcome::Pending(reason)` — the check cannot run yet; print a clear
  reason, never a fake pass.
- `CheckError` — the check failed.
- A panic is caught by the runner and reported as `FAIL`.

## Registry

`prove/registry.tsv` is the source of truth for public checks. Each row has
four tab-separated columns:

```text
id	gate	kind	description
P0-A1	0	acceptance	Clicking 'send test event' ... within 5 seconds
```

The runner skips empty lines and lines whose first non-space character is
`#`. `just prove <phase> <env>` runs every registered check with
`phase <= <phase>`. An earlier gate can never be silently skipped.

## Adding a proof

1. Add the row to `prove/registry.tsv`.
2. Add `pN_<what_the_check_proves>` to `crates/prove/src/checks/p<gate>.rs`
   and register it with `check!`.
3. Run `just prove <gate> local --only <ID>` and watch it fail for the right
   reason.
4. Implement the feature until it passes.
5. Run `just prove-lint`.

`just prove-lint` checks for duplicate registry IDs, duplicate implemented
check IDs, implementations that are not registered, duplicate function names,
and every P0 registry ID having an implementation. When `docs/phases/` is present, it also compares documented
IDs against the registry in both directions. A public checkout without the
planning documents skips only that documentation comparison.

## Environments

Environment settings live in `prove/env/<env>.env` as `KEY=VALUE` lines. The
default environment is `local`. The runner loads the file with `dotenvy`; OS
environment variables win. `GP_ENV` is set to the environment name, and
`BASE_URL` defaults to `http://localhost:8080`.

Local defaults:

```dotenv
BASE_URL=http://localhost:8080
PROVE_EXPECTED_BUILD_SHA=dev
```

- `BASE_URL` — the HTTP endpoint under test.
- `PROVE_EXPECTED_BUILD_SHA` — the SHA the dashboard must render.
- `PROVE_DB_CONTAINER` — optional local Docker container name used by
  `P0-A3`. When unset, `P0-A3` reports `PENDING`.
- `KUBE_NAMESPACE`, `PROD_NAMESPACE` — cluster namespaces for deploy gates.
- `GH_REPO` — the repository used for CI and lead-time checks.

## Results

Every run writes a TSV under `prove/results/`:

```text
prove/results/20261118T120000Z-local-p0.tsv
```

The header is `id`, `status`, `description`, `reason`; one row follows per
selected check. `reason` is empty for `PASS`, the pending explanation for
`PENDING`, and the error text for `FAIL`. Tabs, newlines and carriage returns
are replaced with spaces in every column so each row stays exactly four
tab-separated fields. `prove/results/` is gitignored.

## Writing a good proof

- Assert the user-visible outcome, not an internal implementation detail.
- Prefer an HTTP request, a database query or a Kubernetes read over grepping
  logs.
- Make the failure message say what was expected, what happened and where to
  look.
- A drill must assert the fault actually took effect before asserting
  recovery (see the Phase 12 silent-failure register).
- Never weaken a check to make a run pass. If the check is wrong, change it
  explicitly and explain why.
- A silent-failure check must prove that the failure is detected, not merely
  that the happy path works.
