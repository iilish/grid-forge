# Staging (k3d + Argo CD). From the laptop, port-forward first:
#   kubectl -n staging port-forward svc/gridforge 8080:8080
# Inside the cluster the proof Job sets BASE_URL to the service DNS name.
export BASE_URL="${BASE_URL:-http://localhost:8080}"
export KUBE_NAMESPACE="${KUBE_NAMESPACE:-staging}"
export PROD_NAMESPACE="${PROD_NAMESPACE:-prod}"
export GH_REPO="${GH_REPO:-iilish/grid-forge}"
