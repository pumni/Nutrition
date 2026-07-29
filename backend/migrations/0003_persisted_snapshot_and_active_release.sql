ALTER TABLE analysis.analysis_revision
    ADD COLUMN result_snapshot jsonb,
    ADD COLUMN snapshot_hash text;

ALTER TABLE analysis.analysis_revision
    ADD CONSTRAINT analysis_final_revision_has_snapshot
    CHECK (
        result_status = 'building'
        OR (
            result_snapshot IS NOT NULL
            AND snapshot_hash ~ '^[0-9a-f]{64}$'
        )
    );

CREATE UNIQUE INDEX uq_one_active_catalog_release
    ON catalog.catalog_release ((status))
    WHERE status = 'active';

