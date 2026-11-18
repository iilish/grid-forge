set shell := ["bash", "-euo", "pipefail", "-c"]

default:
    @just --list

# Run the proofs for phases 0..N against an environment: just prove 3 staging
# Add --allow-pending while a phase is under development.
prove phase env="local" *flags:
    prove/prove.sh {{phase}} {{env}} {{flags}}

# List every registered check
prove-list:
    prove/prove.sh --list

# Verify docs, registry and check implementations agree
prove-lint:
    prove/prove.sh --lint

# Print the filled-in AI prompt for a phase: just phase-prompt 3
phase-prompt n:
    @cat docs/prompts/phase-{{n}}.md

# Create the next numbered ADR from the template: just adr "Partition key"
adr title:
    #!/usr/bin/env bash
    set -euo pipefail
    last=$(ls docs/adr | grep -E '^[0-9]{4}-' | sort | tail -1 | cut -c1-4)
    n=$(printf '%04d' $((10#${last:-0000} + 1)))
    slug=$(echo "{{title}}" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9' '-' | sed 's/^-//; s/-$//')
    sed "s/^# ADR NNNN: .*/# ADR $n: {{title}}/; s/YYYY-MM-DD/$(date +%F)/" docs/adr/0000-template.md > "docs/adr/$n-$slug.md"
    echo "created docs/adr/$n-$slug.md"
