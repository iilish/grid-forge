#!/usr/bin/env bash
# Run a command against a disposable PostgreSQL database.
#
#   scripts/with-postgres.sh cargo nextest run -p gridforge-store --features db-tests
#
# The container and its data are removed when the command exits.
# Use set -euxo pipefail to see commands before execution
set -euo pipefail

name="${GRIDFORGE_TEST_POSTGRES_NAME:-gridforge-test-postgres-$$}"
port="${GRIDFORGE_TEST_POSTGRES_PORT:-55432}"
image="${GRIDFORGE_TEST_POSTGRES_IMAGE:-postgres:18-alpine}"

cleanup() {
  docker rm -f "$name" >/dev/null 2>&1 || true
}
trap cleanup EXIT

command -v docker >/dev/null 2>&1 || {
  echo "docker is required for database tests" >&2
  exit 1
}
docker info >/dev/null 2>&1 || {
  echo "Docker daemon is not running; start Docker and retry" >&2
  exit 1
}

docker run --rm -d --name "$name" \
  -e POSTGRES_USER=gridforge \
  -e POSTGRES_PASSWORD=gridforge \
  -e POSTGRES_DB=gridforge \
  -p "${port}:5432" \
  "$image" >/dev/null

ready=0
for _ in $(seq 1 60); do
  if docker exec "$name" pg_isready -U gridforge -d gridforge >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 1
done

if [[ "$ready" != 1 ]]; then
  echo "Postgres did not become ready; container logs:" >&2
  docker logs "$name" >&2 || true
  exit 1
fi

export DATABASE_URL="postgres://gridforge:gridforge@localhost:${port}/gridforge"
"$@"
