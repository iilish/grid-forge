# Staging lifecycle on k3d

Phase 0 staging runs on a local k3d cluster with Argo CD. The cluster uses the
local Docker daemon and survives Docker restarts while the k3d containers
exist; it does not survive deleting the cluster.

## Choose the loop

- Local app loop: `just up` builds the local Dockerfile and runs Docker Compose.
  Use this for uncommitted code.
- GitOps staging loop: this runbook. Argo CD deploys pushed `main` and `deploy`
  refs; uncommitted local changes are not deployed.

## Prerequisites

- Docker
- k3d
- kubectl
- helm
- argocd CLI, for `just argocd-sync` and `just proof-rerun`
- openssl
- a GitHub fine-grained token for proof commit statuses with:
  - Actions: read
  - Commit statuses: write
  - Contents: read

Fine-grained tokens cannot call the Checks API. True GitHub check runs would need a GitHub App installation token; the proof runner posts commit statuses instead.

This runbook assumes the full Argo CD install. The core-only install omits the
API server and UI, which breaks `just argocd-sync` and `just proof-rerun`.

## Create the cluster

```bash
just cluster-up
```

`k3d cluster create` merges kubeconfig and sets the current context to
`k3d-gridforge`. No manual `kubectl` configuration is needed.

```bash
kubectl config current-context
# k3d-gridforge
```

`k3d cluster stop/start` keeps the context. `just cluster-down` removes it.

## Install Argo CD

```bash
just argocd-install
```

Read the initial admin password with:

```bash
kubectl -n argocd get secret argocd-initial-admin-secret \
  -o jsonpath='{.data.password}' | base64 -d
```

## Create namespaces and secrets

Create the namespaces first so the Secrets can be created. `just
argocd-bootstrap` also creates them, but the Secrets must exist before the root
app syncs.

```bash
kubectl create namespace staging
kubectl create namespace prod
```

Create the database secret in each namespace. Use the same password in
`postgres-password` and in the password part of `database-url`.

```bash
DB_PASSWORD="$(openssl rand -hex 32)"
for ns in staging prod; do
  kubectl -n "$ns" create secret generic gridforge-db \
    --from-literal=postgres-password="$DB_PASSWORD" \
    --from-literal=database-url="postgres://gridforge:${DB_PASSWORD}@postgres:5432/gridforge"
done
```

Create the proof token secret in each namespace. The proof Job reads key
`token`.

```bash
export GH_TOKEN="<github-fine-grained-token>"
for ns in staging prod; do
  kubectl -n "$ns" create secret generic gridforge-proof-token \
    --from-literal=token="$GH_TOKEN"
done
```

If the GHCR packages are private, also create an image pull secret and add it to
`imagePullSecrets` in the environment values.

## Bootstrap Argo CD applications

```bash
just argocd-bootstrap
```

This creates the `staging` and `prod` namespaces if needed, applies the
cluster-scoped Postgres StorageClass once, and applies the root app-of-apps.
Argo CD then creates the Postgres and application Applications.

The staging application auto-syncs chart `main` with values from `deploy`. The
prod application is manual.

## Access the services

Argo CD UI:

```bash
just argocd-port-forward 8090
```

Then open `https://localhost:8090`.

GridForge staging:

```bash
RELEASE=gridforge-staging NAMESPACE=staging just port-forward 8080
```

Then open `http://localhost:8080/`.

## Verify staging

```bash
kubectl -n argocd get applications
kubectl -n staging get pods,jobs
kubectl -n staging logs job/gridforge-staging-proof
```

Commit statuses appear in GitHub as `prove/staging` on the build SHA.

The proof Job is kept for 600 seconds after it finishes. If it is gone, run
`just proof-rerun` to recreate it.

## Re-run proofs

```bash
just proof-rerun
```

This triggers an Argo CD sync, which re-runs the PostSync proof Job.

## Stop and start

```bash
k3d cluster stop gridforge
k3d cluster start gridforge
```

Argo CD resumes syncing after the cluster starts. The Postgres PVC is retained.
`kubectl` commands fail while the cluster is stopped.

## Delete

```bash
just cluster-down
```

This removes the cluster and its local volumes. The retained-PVC policy only
protects volumes while the cluster exists.
