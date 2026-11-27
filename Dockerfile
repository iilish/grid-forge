# syntax=docker/dockerfile:1.7

FROM rust:1.99-bookworm AS builder

ARG BUILD_SHA=dev
ENV GRIDFORGE_BUILD_SHA=${BUILD_SHA}

WORKDIR /src
COPY . .

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release -p grid-forge --bin gridforge \
    && cp target/release/gridforge /usr/local/bin/gridforge

FROM debian:bookworm-slim

ARG BUILD_SHA=dev

LABEL org.opencontainers.image.source="https://github.com/iilish/grid-forge" \
      org.opencontainers.image.title="gridforge" \
      org.opencontainers.image.version="${BUILD_SHA}" \
      org.opencontainers.image.revision="${BUILD_SHA}"

RUN apt-get update \
    && apt-get upgrade -y \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 gridforge \
    && useradd --uid 10001 --gid 10001 --system --create-home gridforge

COPY --from=builder /usr/local/bin/gridforge /usr/local/bin/gridforge

USER 10001:10001
EXPOSE 8080

ENTRYPOINT ["gridforge"]
CMD ["serve"]
