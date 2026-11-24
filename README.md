# GridForge

**An operator console and event-driven control plane for distributed energy resources.**

GridForge is a Rust system for operating a fleet of batteries, inverters and meters at customer sites. It ingests device telemetry, keeps an ordered and deduplicated history, shows the fleet's current state, and delivers auditable commands and campaigns across the fleet.

> **Status:** GridForge is under active development. This README describes the intended finished system; see tags and releases for current maturity.

## Quickstart

```bash
git clone https://github.com/iilish/grid-forge.git
cd grid-forge
cp .env.example .env
just up
curl -fsS http://localhost:8080/healthz
curl -fsS http://localhost:8080/readyz
```

Open <http://localhost:8080> for the console. Then run the full local gate (this needs the verification tools below):

```bash
just check-full
```

Stop the stack with `just down`. See [Prerequisites](#prerequisites), [Ports and URLs](#ports-and-urls) and [Configuration](#configuration) for details.

## Prerequisites

Core local development:

| Tool | Tested with | Needed for |
|---|---|---|
| **Rust** | 1.99.0, edition 2024 | Building and testing the workspace. |
| **Docker Engine or Docker Desktop** | 28.3+ with Compose v2 | Postgres, the Kafka-compatible broker and the local stack. |
| **just** | 1.58+ | All project commands. |
| **curl** | Any recent version | Health checks and smoke tests. |

Full verification and cluster workflows:

| Tool | Tested with | Needed for |
|---|---|---|
| **cargo-nextest** | 0.9+ | `just test`. |
| **cargo-deny**, **cargo-audit** | 0.21+, 0.23+ | Dependency checks added to `just lint` when configured. |
| **sqlx-cli** | 0.9+ | Prepared-query checks added to `just lint` once queries exist. |
| **buf** | 1.75+ | Protobuf lint and later breaking-change checks. |
| **yq** | Mike Farah v4.53+ | OpenAPI YAML parse check. |
| **Helm** | 4.3+ | Chart lint and local Kubernetes deployment. |
| **kubectl** | 1.32+ | Inspecting local clusters and proof jobs. |
| **k3d** | 5.9+ | Local Kubernetes without a separate VM. |
| **cosign**, **trivy**, **syft** | 3.1+, 0.76+, 1.57+ | Release signing, scanning and SBOM checks. |
| **Argo CD CLI** | 3.6+ | Debugging sync and application health. |

You need macOS, Linux, or Windows with WSL2. Node and pnpm are **not** required; the UI is server-rendered Rust.

No port conflicts: the stack uses 8080 and 5432 by default; 9092 is added with the telemetry milestone.

## What `just up` starts

`just up`:

1. Checks that Docker is running.
2. Starts Postgres in a container. The Kafka-compatible broker is added with the telemetry milestone.
3. Builds the GridForge image locally.
4. Applies database migrations.
5. Starts the GridForge web and ingest process.
6. Waits until `/readyz` reports ready.

The simulator is **not** started automatically. Run `just simulator` when you want synthetic devices.

**First run:** image pulls and the first Rust build dominate; expect several minutes. Later starts are normally seconds. If startup fails, check `just logs`.

**Data:** Postgres uses a named Docker volume. `just down` stops and removes containers but keeps the data. `just down --volumes` deletes the local database and broker data.

**Healthy first run:**

```bash
curl -fsS http://localhost:8080/healthz   # process is alive
curl -fsS http://localhost:8080/readyz    # dependencies are reachable
curl -fsS http://localhost:8080/metrics   # metrics are exposed
```

The console at <http://localhost:8080> should load. On the first milestone it shows the walking-skeleton dashboard and a test-event form.

## Ports and URLs

| Service | Address | Notes |
|---|---|---|
| **Console and HTTP API** | <http://localhost:8080> | UI, ingest, health and metrics share the listener. |
| **Liveness** | <http://localhost:8080/healthz> | Process health only. |
| **Readiness** | <http://localhost:8080/readyz> | Fails when a required dependency is unavailable. |
| **Metrics** | <http://localhost:8080/metrics> | Prometheus text format. |
| **Postgres** | `localhost:5432` | Database `gridforge`, user `gridforge`, local password only. |
| **Kafka-compatible broker** | `localhost:9092` | Redpanda locally; used from the telemetry milestone onward. |
| **Broker admin API** | `localhost:9644` | Redpanda admin endpoint when enabled. |

There is no separate admin UI. Use `just logs`, the metrics endpoint and the console.

## Configuration

Copy [`.env.example`](.env.example) to `.env`:

```bash
cp .env.example .env
```

`.env` is gitignored. Never reuse these local credentials outside local development.

| Variable | Safe local default | Required when |
|---|---|---|
| `GRIDFORGE_ENV` | `local` | Always; selects environment behavior. |
| `GRIDFORGE_HTTP_ADDR` | `0.0.0.0:8080` | Always; HTTP listener address. |
| `GRIDFORGE_ORG_ID` | `00000000-0000-0000-0000-000000000001` | Always; sentinel org before multi-tenant login. |
| `RUST_LOG` | `info,gridforge=debug` | Always; log filter. |
| `POSTGRES_USER` | `gridforge` | Local Compose only. |
| `POSTGRES_PASSWORD` | `gridforge` | Local Compose only; must be replaced outside local. |
| `POSTGRES_DB` | `gridforge` | Local Compose only. |
| `DATABASE_URL` | `postgres://gridforge:gridforge@localhost:5432/gridforge` | Always; application database connection. |
| `GRIDFORGE_KAFKA_BROKERS` | `localhost:9092` | From the telemetry milestone onward. |
| `GRIDFORGE_SIMULATOR_DEVICES` | `20` | `just simulator`. |
| `GRIDFORGE_SIMULATOR_INTERVAL_MS` | `1000` | `just simulator`. |
| `GRIDFORGE_SIMULATOR_DURATION_SECS` | `0` (run until interrupted) | `just simulator`. |
| `GRIDFORGE_OIDC_ISSUER_URL` | none | When operator login is enabled. |
| `GRIDFORGE_OIDC_CLIENT_ID` | none | When operator login is enabled. |
| `GRIDFORGE_OIDC_CLIENT_SECRET` | none | When operator login is enabled. |

The OIDC values have no safe defaults and must be set manually. Staging and production get secrets from External Secrets or SOPS, never from `.env`.

## Architecture

```mermaid
flowchart LR
  subgraph Edge["Edge / simulation"]
    DEV["Devices and DERs"]
    SIM["Simulator"]
  end

  subgraph CP["GridForge control plane"]
    ING["Ingest API"]
    KAFKA[("Kafka / Redpanda")]
    PROC["Processor"]
    RELAY["Relay"]
    WEB["Web console"]
    DB[("PostgreSQL")]
  end

  IDP["OIDC provider"]
  BROWSER["Operator browser"]
  OBS["Logs, metrics, traces"]

  DEV -->|"telemetry.v1"| ING
  SIM -->|"telemetry.v1"| ING
  ING --> KAFKA
  KAFKA --> PROC
  PROC --> DB
  BROWSER --> WEB
  WEB --> DB
  WEB --> IDP
  RELAY --> DB
  RELAY -->|"commands.v1"| KAFKA
  KAFKA --> DEV
  KAFKA --> SIM
  DEV -->|"command acknowledgement"| ING
  SIM -->|"command acknowledgement"| ING
  ING --> OBS
  PROC --> OBS
  RELAY --> OBS
  WEB --> OBS
```

### Telemetry path

1. A device or simulator posts a versioned Protobuf envelope to the ingest API.
2. Ingest validates the envelope, authenticates the device and publishes it to `telemetry.v1`.
3. The processor writes history idempotently and updates the device's latest state.
4. The console renders the result. Later, SSE pushes live updates.

### Command path

1. An operator submits a command or launches a campaign.
2. The command and an outbox row commit in one Postgres transaction.
3. The relay publishes the command to `commands.v1`.
4. The device executes it and reports an acknowledgement on the telemetry path.
5. The command timeline and audit trail record every transition.

Postgres is the system of record. Kafka is the durable transport between processes, not the source of truth for what an operator sees.

## What it does

- **Fleet visibility.** Latest power, state of charge, voltage, status and last-seen age per device, grouped by site.
- **Durable telemetry.** Partitioned history that survives processor, ingest and database restarts.
- **Correctness under messy data.** Idempotent writes for duplicates, explicit late-data and clock-skew rules, and visible data-quality counters.
- **Device registry.** OIDC login, viewer and operator roles, device lifecycle and audit trails.
- **Commands.** A command state machine with TTL, client idempotency and an auditable timeline from request to acknowledgement.
- **Live operations.** Server-sent events for device state and command timelines, with reconnect and gap detection.
- **Campaigns.** Wave-based fan-out, live progress, automatic pause on failure thresholds, second-operator approval and a real abort.
- **Operability.** Dashboards, traces, SLOs, burn-rate alerts, runbooks, rehearsed rollback and chaos drills.
- **Portable deployment.** The same release runs on a local cluster, on Kubernetes, and later on AWS.

## Reliability and safety properties

- **At-least-once delivery with idempotent effects.** Duplicates are expected on the wire and deduplicated at the database. Consumers commit offsets only after the durable write.
- **No dual writes.** State changes and their outbox rows commit together.
- **No silent data loss.** Unknown schema versions are parked and counted. Dropped events, late data and clock skew are visible.
- **Build once, promote the same bytes.** Images are built once, signed, scanned and deployed by digest.
- **Schema evolution is safe.** Migrations are forward-only expand, migrate, contract. Version N of the app runs against schema N and N+1.
- **Rollback is rehearsed.** Every gate proves a rollback to the previous digest and records the time.
- **Tenancy is explicit.** Every table carries `org_id`, every repository call takes an org scope, and the database enforces isolation.
- **Proofs are the definition of done.** Every acceptance check and silent-failure detection has a stable ID and a runnable check.

## How to use GridForge

### As an operator

1. Sign in with your organization's OIDC provider.
2. Browse devices by site and inspect latest state and history.
3. Send a command and follow it from Pending to Applied, Rejected or Expired.
4. Launch a campaign in waves, watch progress and abort if the failure threshold is crossed.
5. Review the audit trail for every change.

Operators do not need shell access to the cluster.

### As a developer

Start with the Quickstart. The commands below are the stable developer interface. Commands are introduced by the milestone that needs them; once documented here, their names do not change without a deprecation note.

#### `just up`

Starts Postgres, applies migrations and starts GridForge. The Kafka-compatible broker is added with the telemetry milestone. Requires Docker. Creates a named data volume. It does not start the simulator. Success: `/healthz` and `/readyz` return 200 and the console loads. Inspect startup with `just logs`.

#### `just down`

Stops and removes local containers. Keeps Postgres and broker volumes. Use `just down --volumes` to delete local data as well.

#### `just logs`

Follows logs from the local stack. `just logs` follows everything; `just logs postgres` follows one service.

#### `just build`

Builds every workspace member with `cargo build --workspace`. Runs build scripts, including Protobuf code generation for `gridforge-contracts`. Success: exit code 0 and `target/debug/` contains the generated code and binaries. It does not run tests.

#### `just check`

Runs the fast local gate: `just lint` followed by `just test`. Use it while iterating. It needs no Docker or Postgres and does not modify files.

#### `just check-full`

Runs the full pre-PR and CI gate: `just check` followed by `just test-db`. Use it before opening or updating a pull request and before a release. It starts a disposable Postgres container, runs the database migration tests, and removes the container. It does not touch local stack data.

Success: exit code 0 with every static check, unit/contract test and database test passing. GitHub Actions runs this same command on pull requests, `main` and release tags.

#### `just test`

Runs unit and contract tests with nextest. Database-backed migration and persistence tests are separate in `just test-db`, so `just test` stays fast and does not need Postgres.

```bash
cargo nextest run -p contracts
cargo nextest run envelope
```

Success: exit code 0. Tests never modify your development data.

#### `just test-db`

Starts a disposable Postgres container, exports `DATABASE_URL`, runs the SQLx migration and walking-skeleton persistence tests, and removes the container. Requires Docker. It does not touch local stack data.

```bash
just test-db
```

Success: six database tests pass: four migration tests (fresh schema, idempotent second run, `org_id uuid not null`, down migration removing `events`) and two persistence tests (one inserted event, newest 20 ordering).

#### `just lint`

Runs static checks without modifying files:

- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `buf lint` and `buf format --diff --exit-code`
- OpenAPI YAML parse with `yq`
- `just prove-lint`
- `cargo xtask check-layering` (fails on violations)

Dependency audits, SQLx prepared-query checks, proto breaking checks and Helm/Kubernetes validation are added when those artifacts exist. Success: exit code 0. Fix formatting with `cargo fmt --all`; fix contract or manifest errors at the source. The full lint set needs the tools listed under [Prerequisites](#prerequisites).

#### `just simulator`

Creates `GRIDFORGE_SIMULATOR_DEVICES` synthetic devices, sends telemetry every `GRIDFORGE_SIMULATOR_INTERVAL_MS`, listens for commands and acknowledges them. Duration `0` runs until interrupted. Stop with Ctrl-C. It writes to the local database; use `just down --volumes` to reset local data.

#### `just prove 0 local`

Runs the proof suite for gate 0 against the local stack. Gate 0 is the walking skeleton: a test event travels through ingest and Postgres and appears on the dashboard; health, readiness, dropped-event detection and rollback evidence are checked.

Proof gates are numbered delivery milestones, not public API versions. The runner calls them "phases" in its output; the public contract is the check registry.

A run writes `prove/results/<timestamp>-local-p0.tsv`. Success: exit code 0 with every selected check `PASS`. `PENDING` exits 2; only local development may pass `--allow-pending`. Deploy gates never allow it.

#### `just prove-list`

Prints every registered check from [`prove/registry.tsv`](prove/registry.tsv) with its ID, gate, kind, function name and description. To add a proof, add a registry row and a `pN_<what_the_check_proves>` function in `crates/prove/src/checks/pN.rs`; see [`crates/prove/README.md`](crates/prove/README.md).

#### `just prove-lint`

Checks that the registry and check implementations agree: duplicate IDs, implementations that are not registered, and, when the local planning documents are present, IDs that appear in one place but not the other. Success: a `lint ok` line and exit code 0. Fix by adding the missing registry row, renaming or removing the stray implementation, or syncing the planning document.

## Deployment model

| Environment | Runtime | Purpose |
|---|---|---|
| **local** | Docker Compose plus k3d | Inner loop. |
| **preview** | Ephemeral kind cluster in CI | Per-pull-request end-to-end proof. |
| **staging** | Single-node k3s | Automatic deployment from `main`; full proof suite. |
| **prod** | Same chart, promoted digest | Deliberate promotion; safe production proofs. |
| **AWS** | EKS, RDS and MSK or Redpanda | Portability proof and production-shaped runtime. |

A Helm chart packages the services, Argo CD runs in-cluster and syncs from the public repository, and environment values pin the image by digest. Hosted CI never reaches into the cluster; in-cluster proof jobs report their results back to GitHub.

## How it is built

- **Rust workspace.** Shared contracts and domain rules, with separate crates for ingest, processor, relay, web and the simulator.
- **Axum and Tower.** Server-rendered HTML with Askama, HTMX for partial refresh and server-sent events for live operations.
- **Postgres.** SeaORM for entities and ordinary CRUD; SQLx for migrations and hot paths where explicit SQL is clearer or measurably faster.
- **Kafka-compatible transport.** Redpanda locally; the contract is versioned Protobuf topics.
- **Kubernetes.** One Helm chart, Argo CD, digest-based promotion and signed images.
- **Proof runner.** Rust checks with stable IDs, recorded results and a registry that must match the implementations.

## Repository layout

```
crates/{contracts,domain,store,ingest,processor,relay,web,simulator,bench,xtask,prove}
crates/simulator               gridforge-simulator Envelope v1 load generator
deploy/helm/gridforge          one chart, per-service values
deploy/envs/{staging,prod}     environment values and image digests
infra/                         OpenTofu, from the AWS milestone
ops/runbooks/                  one runbook per alert and drill
bench/results/                 committed raw benchmark output
prove/                         registry, environments, results
scripts/                       disposable test infrastructure
.github/workflows/             CI gates
Dockerfile                     release image build
compose.yaml                   local stack: Postgres, migrations, app
justfile
```

## Non-goals

GridForge is not a safety-certified real-time control system, device firmware, a general-purpose IoT platform, a multi-region active-active system, or a microservice showcase. It is a focused operator console and control plane for DER fleets.
