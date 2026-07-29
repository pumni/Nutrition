ALTER TABLE analysis.clarification_question
    ADD COLUMN context_payload jsonb NOT NULL DEFAULT '{}';

CREATE OR REPLACE FUNCTION ops.enforce_meal_analysis_status_transition()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.status = OLD.status THEN
        RETURN NEW;
    END IF;

    IF (OLD.status = 'received' AND NEW.status = 'parsing')
       OR (OLD.status = 'parsing' AND NEW.status = 'resolving')
       OR (OLD.status = 'resolving'
           AND NEW.status IN ('completed', 'needs_clarification', 'insufficient_evidence'))
       OR (OLD.status = 'needs_clarification'
           AND NEW.status IN ('resolving', 'completed', 'abandoned'))
       OR (OLD.status = 'completed' AND NEW.status IN ('confirmed', 'corrected'))
       OR (OLD.status = 'corrected' AND NEW.status = 'completed')
       OR (OLD.status = 'insufficient_evidence' AND NEW.status = 'corrected') THEN
        RETURN NEW;
    END IF;

    RAISE EXCEPTION 'invalid meal analysis status transition: % -> %', OLD.status, NEW.status;
END;
$$;

CREATE TRIGGER meal_analysis_status_transition
    BEFORE UPDATE OF status ON analysis.meal_analysis
    FOR EACH ROW EXECUTE FUNCTION ops.enforce_meal_analysis_status_transition();

CREATE OR REPLACE FUNCTION ops.reject_answered_clarification_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status <> 'open' THEN
        RAISE EXCEPTION 'closed clarification question % is immutable', OLD.id;
    END IF;
    IF TG_OP = 'UPDATE' AND NEW.status NOT IN ('answered', 'expired', 'cancelled') THEN
        RAISE EXCEPTION 'open clarification question % may only be closed', OLD.id;
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE TRIGGER closed_clarification_question_immutable
    BEFORE UPDATE OR DELETE ON analysis.clarification_question
    FOR EACH ROW EXECUTE FUNCTION ops.reject_answered_clarification_mutation();

CREATE OR REPLACE FUNCTION ops.reject_clarification_answer_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'clarification answers are append-only';
END;
$$;

CREATE TRIGGER clarification_answer_append_only
    BEFORE UPDATE OR DELETE ON analysis.clarification_answer
    FOR EACH ROW EXECUTE FUNCTION ops.reject_clarification_answer_mutation();

CREATE OR REPLACE FUNCTION ops.reject_analysis_correction_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    RAISE EXCEPTION 'analysis corrections are append-only';
END;
$$;

CREATE TRIGGER analysis_correction_append_only
    BEFORE UPDATE OR DELETE ON app.analysis_correction
    FOR EACH ROW EXECUTE FUNCTION ops.reject_analysis_correction_mutation();
