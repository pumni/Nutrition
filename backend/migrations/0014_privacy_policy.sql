ALTER TABLE analysis.meal_analysis
    ADD CONSTRAINT meal_analysis_raw_text_disabled
    CHECK (raw_text_ciphertext IS NULL);

CREATE OR REPLACE FUNCTION ops.privacy_purge_scope_is_set()
RETURNS boolean
LANGUAGE sql
STABLE
AS $$
    SELECT NULLIF(btrim(current_setting('app.privacy_purge_user_id', true)), '') IS NOT NULL;
$$;

CREATE OR REPLACE FUNCTION ops.privacy_purge_user_id()
RETURNS uuid
LANGUAGE plpgsql
STABLE
AS $$
DECLARE
    raw_user_id text;
BEGIN
    raw_user_id := NULLIF(btrim(current_setting('app.privacy_purge_user_id', true)), '');
    IF raw_user_id IS NULL THEN
        RETURN NULL;
    END IF;
    BEGIN
        RETURN raw_user_id::uuid;
    EXCEPTION
        WHEN invalid_text_representation THEN
            RETURN NULL;
    END;
END;
$$;

CREATE OR REPLACE FUNCTION ops.privacy_purge_user_matches(target_user_id uuid)
RETURNS boolean
LANGUAGE sql
STABLE
AS $$
    SELECT target_user_id IS NOT NULL
       AND target_user_id = ops.privacy_purge_user_id();
$$;

CREATE OR REPLACE FUNCTION ops.reject_final_revision_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    target_user_id uuid;
BEGIN
    IF TG_OP = 'DELETE' AND ops.privacy_purge_scope_is_set() THEN
        SELECT meal.user_id
          INTO target_user_id
          FROM analysis.meal_analysis meal
         WHERE meal.id = OLD.meal_analysis_id;
        IF NOT ops.privacy_purge_user_matches(target_user_id) THEN
            RAISE EXCEPTION 'privacy purge scope does not own analysis revision %', OLD.id;
        END IF;
    END IF;

    IF OLD.result_status <> 'building' THEN
        IF TG_OP = 'DELETE'
           AND ops.privacy_purge_user_matches(target_user_id) THEN
            RETURN OLD;
        END IF;
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
    target_user_id uuid;
BEGIN
    target_revision_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.revision_id ELSE NEW.revision_id END;
    IF TG_OP = 'DELETE' AND ops.privacy_purge_scope_is_set() THEN
        SELECT meal.user_id
          INTO target_user_id
          FROM analysis.analysis_revision revision
          JOIN analysis.meal_analysis meal
            ON meal.id = revision.meal_analysis_id
         WHERE revision.id = target_revision_id;
        IF NOT ops.privacy_purge_user_matches(target_user_id) THEN
            RAISE EXCEPTION 'privacy purge scope does not own revision child %', target_revision_id;
        END IF;
    END IF;
    IF EXISTS (
        SELECT 1
        FROM analysis.analysis_revision revision
        WHERE revision.id = target_revision_id
          AND revision.result_status <> 'building'
    ) THEN
        IF TG_OP = 'DELETE'
           AND ops.privacy_purge_user_matches(target_user_id) THEN
            RETURN OLD;
        END IF;
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
    target_user_id uuid;
BEGIN
    target_item_id := CASE WHEN TG_OP = 'DELETE' THEN OLD.item_id ELSE NEW.item_id END;
    SELECT item.revision_id
      INTO target_revision_id
      FROM analysis.analysis_item item
     WHERE item.id = target_item_id;
    IF TG_OP = 'DELETE' AND ops.privacy_purge_scope_is_set() THEN
        SELECT meal.user_id
          INTO target_user_id
          FROM analysis.analysis_revision revision
          JOIN analysis.meal_analysis meal
            ON meal.id = revision.meal_analysis_id
         WHERE revision.id = target_revision_id;
        IF NOT ops.privacy_purge_user_matches(target_user_id) THEN
            RAISE EXCEPTION 'privacy purge scope does not own item child %', target_item_id;
        END IF;
    END IF;
    IF EXISTS (
        SELECT 1
        FROM analysis.analysis_revision revision
        WHERE revision.id = target_revision_id
          AND revision.result_status <> 'building'
    ) THEN
        IF TG_OP = 'DELETE'
           AND ops.privacy_purge_user_matches(target_user_id) THEN
            RETURN OLD;
        END IF;
        RAISE EXCEPTION 'children of final analysis revision % are immutable', target_revision_id;
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE OR REPLACE FUNCTION ops.reject_answered_clarification_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    target_user_id uuid;
BEGIN
    IF TG_OP = 'DELETE' AND ops.privacy_purge_scope_is_set() THEN
        SELECT meal.user_id
          INTO target_user_id
          FROM analysis.clarification_question question
          JOIN analysis.analysis_revision revision
            ON revision.id = question.analysis_revision_id
          JOIN analysis.meal_analysis meal
            ON meal.id = revision.meal_analysis_id
         WHERE question.id = OLD.id;
        IF NOT ops.privacy_purge_user_matches(target_user_id) THEN
            RAISE EXCEPTION 'privacy purge scope does not own clarification %', OLD.id;
        END IF;
    END IF;
    IF OLD.status <> 'open' THEN
        IF TG_OP = 'DELETE'
           AND ops.privacy_purge_user_matches(target_user_id) THEN
            RETURN OLD;
        END IF;
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
DECLARE
    target_user_id uuid;
BEGIN
    IF TG_OP = 'DELETE' AND ops.privacy_purge_scope_is_set() THEN
        SELECT meal.user_id
          INTO target_user_id
          FROM analysis.clarification_answer answer
          JOIN analysis.clarification_question question
            ON question.id = answer.question_id
          JOIN analysis.analysis_revision revision
            ON revision.id = question.analysis_revision_id
          JOIN analysis.meal_analysis meal
            ON meal.id = revision.meal_analysis_id
         WHERE answer.id = OLD.id;
        IF NOT ops.privacy_purge_user_matches(target_user_id) THEN
            RAISE EXCEPTION 'privacy purge scope does not own clarification answer %', OLD.id;
        END IF;
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'clarification answers are append-only';
END;
$$;

CREATE OR REPLACE FUNCTION ops.reject_analysis_correction_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    target_user_id uuid;
BEGIN
    IF TG_OP = 'DELETE' AND ops.privacy_purge_scope_is_set() THEN
        SELECT meal.user_id
          INTO target_user_id
          FROM analysis.meal_analysis meal
         WHERE meal.id = OLD.meal_analysis_id;
        IF NOT ops.privacy_purge_user_matches(target_user_id) THEN
            RAISE EXCEPTION 'privacy purge scope does not own correction %', OLD.id;
        END IF;
        RETURN OLD;
    END IF;
    RAISE EXCEPTION 'analysis corrections are append-only';
END;
$$;
