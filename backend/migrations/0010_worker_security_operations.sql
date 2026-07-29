ALTER TABLE ops.outbox_event
    ADD COLUMN delivery_attempts integer NOT NULL DEFAULT 0
        CHECK (delivery_attempts >= 0),
    ADD COLUMN available_at timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN locked_at timestamptz,
    ADD COLUMN locked_by text,
    ADD COLUMN last_error text,
    ADD COLUMN dead_at timestamptz;

CREATE INDEX ix_outbox_delivery_claim
    ON ops.outbox_event (available_at, created_at)
    WHERE published_at IS NULL AND dead_at IS NULL;

CREATE TABLE ops.audit_event (
    id uuid PRIMARY KEY,
    actor_id uuid,
    action text NOT NULL,
    target_type text NOT NULL,
    target_id uuid NOT NULL,
    metadata jsonb NOT NULL DEFAULT '{}',
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX ix_audit_target
    ON ops.audit_event (target_type, target_id, created_at);

CREATE OR REPLACE FUNCTION ops.reject_analysis_owner_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.user_id IS DISTINCT FROM OLD.user_id THEN
        RAISE EXCEPTION 'analysis owner is immutable';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER meal_analysis_owner_immutable
    BEFORE UPDATE OF user_id ON analysis.meal_analysis
    FOR EACH ROW EXECUTE FUNCTION ops.reject_analysis_owner_mutation();

ALTER TABLE ops.job
    ADD CONSTRAINT job_running_has_lease
    CHECK (
        status <> 'running'
        OR (locked_at IS NOT NULL AND locked_by IS NOT NULL)
    );
