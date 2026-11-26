# syntax=docker/dockerfile:1.7

FROM --platform=$BUILDPLATFORM golang:1.27.4-bookworm AS tools

ARG TARGETARCH
ARG COSIGN_VERSION=v3.1.6
ARG CRANE_VERSION=v0.22.1

RUN mkdir -p /out \
    && CGO_ENABLED=0 GOOS=linux GOARCH=${TARGETARCH} \
        go install github.com/sigstore/cosign/v3/cmd/cosign@${COSIGN_VERSION} \
    && CGO_ENABLED=0 GOOS=linux GOARCH=${TARGETARCH} \
        go install github.com/google/go-containerregistry/cmd/crane@${CRANE_VERSION} \
    && for tool in cosign crane; do \
        if [ -x "$(go env GOPATH)/bin/linux_${TARGETARCH}/${tool}" ]; then \
            cp "$(go env GOPATH)/bin/linux_${TARGETARCH}/${tool}" "/out/${tool}"; \
        else \
            cp "$(go env GOPATH)/bin/${tool}" "/out/${tool}"; \
        fi; \
    done

FROM debian:bookworm-slim

ARG TARGETARCH
ARG BUILD_SHA=dev
ARG KUBECTL_VERSION=v1.37.3

LABEL org.opencontainers.image.source="https://github.com/iilish/grid-forge" \
      org.opencontainers.image.title="gridforge-proof" \
      org.opencontainers.image.version="${BUILD_SHA}" \
      org.opencontainers.image.revision="${BUILD_SHA}"

RUN apt-get update \
    && apt-get install -y --no-install-recommends bash ca-certificates curl jq \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 gridforge \
    && useradd --uid 10001 --gid 10001 --system --create-home gridforge

RUN set -eux; \
    base="https://dl.k8s.io/release/${KUBECTL_VERSION}/bin/linux/${TARGETARCH}"; \
    curl -fsSLo /usr/local/bin/kubectl "${base}/kubectl"; \
    curl -fsSLo /tmp/kubectl.sha256 "${base}/kubectl.sha256"; \
    echo "$(cat /tmp/kubectl.sha256)  /usr/local/bin/kubectl" | sha256sum -c -; \
    chmod 0755 /usr/local/bin/kubectl; \
    rm /tmp/kubectl.sha256

COPY --from=tools /out/cosign /usr/local/bin/cosign
COPY --from=tools /out/crane /usr/local/bin/crane

ENV HOME=/tmp
WORKDIR /tmp
USER 10001:10001

CMD ["/bin/bash"]
