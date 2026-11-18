set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

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
