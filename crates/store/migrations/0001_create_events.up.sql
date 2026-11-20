CREATE TABLE events (
    id             uuid        PRIMARY KEY,
    org_id         uuid        NOT NULL,
    device_id      text        NOT NULL CHECK (device_id <> ''),
    boot_count     bigint      NOT NULL CHECK (boot_count >= 0),
    seq            bigint      NOT NULL CHECK (seq >= 0),
    schema_ver     integer     NOT NULL CHECK (schema_ver >= 1),
    device_ts      timestamptz NOT NULL,
    ingest_ts      timestamptz NOT NULL,
    persisted_ts   timestamptz NOT NULL DEFAULT now(),
    payload        bytea       NOT NULL,
    payload_bytes  integer     NOT NULL CHECK (payload_bytes >= 0),
    payload_type   text        NOT NULL CHECK (payload_type <> ''),
    CONSTRAINT events_payload_size_matches CHECK (payload_bytes = octet_length(payload))
);

CREATE INDEX events_recent_idx
    ON events (org_id, ingest_ts DESC, id DESC);
