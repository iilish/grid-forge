set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

# Build every workspace member
build:
    cargo build --workspace

# Fast local gate: static checks plus unit and contract tests; no Docker
check: lint test

# Full pre-PR and CI gate: fast gate plus disposable Postgres tests
check-full: check test-db

# Static checks; does not run tests
lint: fmt-check clippy-check contracts-check openapi-check prove-lint layering-check

# Check formatting without changing files
fmt-check:
    cargo fmt --all --check

# Clippy with warnings denied
clippy-check:
    cargo clippy --workspace --all-targets -- -D warnings

# Protobuf lint and format check
contracts-check:
    buf lint
    buf format --diff --exit-code

# YAML/OpenAPI parse smoke check
openapi-check:
    yq eval '.' crates/contracts/openapi/openapi.yaml > /dev/null

# Layering scan; fails on violations
layering-check:
    cargo xtask check-layering

# All tests via nextest
test:
    cargo nextest run --workspace

# Database integration tests against a disposable Postgres
test-db:
    scripts/with-postgres.sh bash -c 'cargo nextest run -p gridforge-store --features db-tests --test migrations && cargo nextest run -p grid-forge --features db-tests --test walking_skeleton'

# Run the proofs for gates 0..N against an environment: just prove 3 staging
# Gates are delivery milestones; the runner calls them phases in its output.
# Add --allow-pending while a gate is under development.
prove phase env="local" *flags:
    prove/prove.sh {{phase}} {{env}} {{flags}}

# List every registered check
prove-list:
    prove/prove.sh --list

# Verify registry and check implementations agree; also checks local planning docs when present
prove-lint:
    prove/prove.sh --lint

# Maintainer-only: print the filled-in AI prompt for a gate (requires local docs)
phase-prompt n:
    @cat docs/prompts/phase-{{n}}.md

# Maintainer-only: create the next ADR from the template (requires local docs)
adr title:
    #!/usr/bin/env bash
    set -euo pipefail
    last=$(ls docs/adr | grep -E '^[0-9]{4}-' | sort | tail -1 | cut -c1-4)
    n=$(printf '%04d' $((10#${last:-0000} + 1)))
    slug=$(echo "{{title}}" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-//; s/-$//')
    sed "s/^# ADR NNNN: .*/# ADR $n: {{title}}/; s/YYYY-MM-DD/$(date +%F)/" docs/adr/0000-template.md > "docs/adr/$n-$slug.md"
    echo "created docs/adr/$n-$slug.md"
