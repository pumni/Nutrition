ALTER TABLE analysis.meal_analysis
    ADD CONSTRAINT meal_analysis_raw_text_disabled
    CHECK (raw_text_ciphertext IS NULL);

CREATE OR REPLACE FUNCTION ops.privacy_purge_enabled()
RETURNS boolean
LANGUAGE sql
STABLE
AS $$
    SELECT COALESCE(current_setting('app.privacy_purge', true), 'false') = 'true';
$$;

CREATE OR REPLACE FUNCTION ops.reject_final_revision_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF ops.privacy_purge_enabled() THEN
        RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
    END IF;
    IF OLD.result_status <> 'building' THEN
        RAISE EXCEPTION 'final analysis revision % is immutable', OLD.id;
    END IF;
    IF TG_OP = 'UPDATE'
       AND NEW.result_status = 'building'
       AND NEW IS DISTINCT FROM OLD THEN
        RAISE EXCEPTION 'building revision metadata may only be finalized';
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE OR REPLACE FUNCTION ops.reject_final_revision_child_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    target_revision_id uuid;
BEGIN
    IF ops.privacy_purge_enabled() THEN
        RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
    END IF;
    target_revision_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.revision_id ELSE NEW.revision_id END;
    IF EXISTS (
        SELECT 1
        FROM analysis.analysis_revision revision
        WHERE revision.id = target_revision_id
          AND revision.result_status <> 'building'
    ) THEN
        RAISE EXCEPTION 'children of final analysis revision % are immutable', target_revision_id;
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE OR REPLACE FUNCTION ops.reject_final_item_child_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    target_item_id uuid;
    target_revision_id uuid;
BEGIN
    IF ops.privacy_purge_enabled() THEN
        RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
    END IF;
    target_item_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.item_id ELSE NEW.item_id END;
    SELECT item.revision_id
      INTO target_revision_id
      FROM analysis.analysis_item item
     WHERE item.id = target_item_id;
    IF EXISTS (
        SELECT 1
        FROM analysis.analysis_revision revision
        WHERE revision.id = target_revision_id
          AND revision.result_status <> 'building'
    ) THEN
        RAISE EXCEPTION 'children of final analysis revision % are immutable', target_revision_id;
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE OR REPLACE FUNCTION ops.reject_answered_clarification_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF ops.privacy_purge_enabled() THEN
        RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
    END IF;
    IF OLD.status <> 'open' THEN
        RAISE EXCEPTION 'closed clarification question % is immutable', OLD.id;
    END IF;
    IF TG_OP = 'UPDATE' AND NEW.status NOT IN ('answered', 'expired', 'cancelled') THEN
        RAISE EXCEPTION 'open clarification question % may only be closed', OLD.id;
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE OR REPLACE FUNCTION ops.reject_clarification_answer_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF ops.privacy_purge_enabled() THEN
        RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
    END IF;
    RAISE EXCEPTION 'clarification answers are append-only';
END;
$$;

CREATE OR REPLACE FUNCTION ops.reject_analysis_correction_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF ops.privacy_purge_enabled() THEN
        RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
    END IF;
    RAISE EXCEPTION 'analysis corrections are append-only';
END;
$$;
