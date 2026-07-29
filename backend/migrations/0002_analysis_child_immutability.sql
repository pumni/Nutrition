CREATE OR REPLACE FUNCTION ops.reject_final_item_child_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    target_item_id uuid;
    target_revision_id uuid;
BEGIN
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

CREATE TRIGGER resolution_candidate_final_revision_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON analysis.resolution_candidate
    FOR EACH ROW EXECUTE FUNCTION ops.reject_final_item_child_mutation();

CREATE TRIGGER item_nutrient_result_final_revision_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON analysis.item_nutrient_result
    FOR EACH ROW EXECUTE FUNCTION ops.reject_final_item_child_mutation();

CREATE TRIGGER revision_total_final_revision_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON analysis.revision_nutrient_total
    FOR EACH ROW EXECUTE FUNCTION ops.reject_final_revision_child_mutation();

