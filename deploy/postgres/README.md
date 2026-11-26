# Postgres

Phase 0 runs one Postgres StatefulSet per environment namespace. It is applied and owned separately from the GridForge Helm chart so app releases never churn the database.

The manifest uses the `gridforge-postgres` StorageClass with `reclaimPolicy: Retain` and the k3s local-path provisioner. The PVC survives StatefulSet deletion. The StorageClass is cluster-scoped and applied once outside the per-environment Argo CD Applications, so staging and prod do not fight over it.

## Secret

The manifest expects a Secret named `gridforge-db` in the target namespace with two keys:

- `postgres-password`: password for the `gridforge` database user.
- `database-url`: `postgres://gridforge:<password>@postgres:5432/gridforge`.

Create it out-of-band. Do not commit it.

```bash
DB_PASSWORD="$(openssl rand -hex 32)"

kubectl -n staging create secret generic gridforge-db \
  --from-literal=postgres-password="$DB_PASSWORD" \
  --from-literal=database-url="postgres://gridforge:${DB_PASSWORD}@postgres:5432/gridforge"
```

## Apply

Apply the cluster-scoped StorageClass once:

```bash
kubectl apply -f deploy/postgres/storageclass.yaml
```

Argo CD owns the namespace-scoped StatefulSet and Service through `deploy/argocd/apps`. For a manual fallback:

```bash
kubectl kustomize deploy/postgres | kubectl -n staging apply -f -
kubectl -n staging rollout status statefulset/postgres
```

Use `prod` instead of `staging` for the production namespace.
