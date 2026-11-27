# Argo CD applications

`deploy/argocd/apps` contains the Argo CD Applications. `deploy/argocd/root.yaml` is the app-of-apps and points at that directory.

The Postgres Applications point at `deploy/postgres` on `main`. Staging syncs automatically with self-heal; prod syncs manually. The cluster-scoped `gridforge-postgres` StorageClass is applied once outside these Applications, so they never fight over the same resource.

The app Applications use two sources:

- `gridforge-staging`: chart from `main`, values from `deploy`, automatic sync.
- `gridforge-prod`: chart and values from `main`, manual sync.

These Applications assume the full Argo CD install, including the API server and UI. A core-only install would break `just argocd-sync` and `just proof-rerun`.

Each app release runs a PostSync Helm hook Job that invokes the proof runner and posts a GitHub commit status. The Job needs the `gridforge-proof-token` Secret in the target namespace.

In the root app-of-apps, the Postgres Applications use sync-wave `0` and the app Applications use sync-wave `1`, so Argo waits for the Postgres Applications to become healthy before syncing the app Applications. Inside the app Application, the migration Job runs as a `PreSync` hook and the proof Job as a `PostSync` hook.

## Bootstrap manually

Argo CD must already be installed in the `argocd` namespace.

Create the namespaces, the Postgres Secret and the proof token Secret in each target namespace, and apply the cluster-scoped StorageClass:

```bash
kubectl create namespace argocd --dry-run=client -o yaml | kubectl apply -f -
kubectl create namespace staging --dry-run=client -o yaml | kubectl apply -f -
kubectl create namespace prod --dry-run=client -o yaml | kubectl apply -f -

STAGING_PG_PASSWORD="$(openssl rand -hex 32)"
kubectl -n staging create secret generic gridforge-db \
  --from-literal=postgres-password="$STAGING_PG_PASSWORD" \
  --from-literal=database-url="postgres://gridforge:${STAGING_PG_PASSWORD}@postgres:5432/gridforge"

PROD_PG_PASSWORD="$(openssl rand -hex 32)"
kubectl -n prod create secret generic gridforge-db \
  --from-literal=postgres-password="$PROD_PG_PASSWORD" \
  --from-literal=database-url="postgres://gridforge:${PROD_PG_PASSWORD}@postgres:5432/gridforge"

for ns in staging prod; do
  kubectl -n "$ns" create secret generic gridforge-proof-token \
    --from-literal=token="$GH_TOKEN"
done

kubectl apply -f deploy/postgres/storageclass.yaml
```

Then apply the root app-of-apps:

```bash
kubectl apply -f deploy/argocd/root.yaml
```

The root Application creates the Postgres and app Applications. The phase-gate
Application is added separately when the phase gate is wired.
