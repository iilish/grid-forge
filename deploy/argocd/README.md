# Argo CD applications

`deploy/argocd/apps` contains the Argo CD Applications. `deploy/argocd/root.yaml` will point here once the Argo CD bootstrap lands.

The Postgres Applications are already defined here and point at `deploy/postgres` on `main`. Staging syncs automatically with self-heal; prod syncs manually. The cluster-scoped `gridforge-postgres` StorageClass is applied once outside these Applications, so they never fight over the same resource. The Argo CD bootstrap adds the app, proof and phase-gate Applications to this directory without duplicating the Postgres Applications.

Sync waves are scoped to one Application. Postgres-before-app ordering must be expressed at the app-of-apps level or handled by Argo health checks and migration retry; the app Application does not automatically wait for the Postgres Application.

## Bootstrap manually

Argo CD must already be installed in the `argocd` namespace.

Create the namespaces, the Postgres Secret in each target namespace, and the cluster-scoped StorageClass:

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

kubectl apply -f deploy/postgres/storageclass.yaml
```

Then apply the Applications. Until `deploy/argocd/root.yaml` exists, apply the directory directly:

```bash
kubectl apply -k deploy/argocd/apps
```

Once the root app-of-apps exists, apply it instead; it points at this directory.
