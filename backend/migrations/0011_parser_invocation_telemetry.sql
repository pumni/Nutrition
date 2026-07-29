CREATE TABLE ops.parser_invocation (
    id uuid PRIMARY KEY,
    provider text NOT NULL,
    model text NOT NULL,
    prompt_version text NOT NULL,
    schema_version text NOT NULL,
    latency_ms bigint NOT NULL CHECK (latency_ms >= 0),
    retry_count integer NOT NULL CHECK (retry_count BETWEEN 0 AND 1),
    input_tokens bigint CHECK (input_tokens IS NULL OR input_tokens >= 0),
    output_tokens bigint CHECK (output_tokens IS NULL OR output_tokens >= 0),
    output_sha256 text CHECK (
        output_sha256 IS NULL OR output_sha256 ~ '^[0-9a-f]{64}$'
    ),
    status text NOT NULL CHECK (status IN ('succeeded', 'failed')),
    error_code text,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (
        (status = 'succeeded' AND error_code IS NULL AND output_sha256 IS NOT NULL)
        OR (status = 'failed' AND error_code IS NOT NULL)
    )
);

CREATE INDEX ix_parser_invocation_provider_created
    ON ops.parser_invocation (provider, model, created_at);
