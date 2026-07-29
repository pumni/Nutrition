CREATE OR REPLACE FUNCTION ops.reject_released_food_name_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF EXISTS (
        SELECT 1
          FROM catalog.catalog_release_food_name membership
          JOIN catalog.catalog_release release
            ON release.id = membership.catalog_release_id
         WHERE membership.food_name_id = OLD.id
           AND release.status IN ('active', 'superseded')
    ) THEN
        RAISE EXCEPTION 'food name % belongs to a published catalog release', OLD.id;
    END IF;

    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE TRIGGER released_food_name_immutable
    BEFORE UPDATE OR DELETE ON catalog.food_name
    FOR EACH ROW EXECUTE FUNCTION ops.reject_released_food_name_mutation();

CREATE OR REPLACE FUNCTION ops.reject_released_portion_observation_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF EXISTS (
        SELECT 1
          FROM catalog.catalog_release_portion_observation membership
          JOIN catalog.catalog_release release
            ON release.id = membership.catalog_release_id
         WHERE membership.portion_observation_id = OLD.id
           AND release.status IN ('active', 'superseded')
    ) THEN
        RAISE EXCEPTION 'portion observation % belongs to a published catalog release', OLD.id;
    END IF;

    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE TRIGGER released_portion_observation_immutable
    BEFORE UPDATE OR DELETE ON composition.portion_observation
    FOR EACH ROW EXECUTE FUNCTION ops.reject_released_portion_observation_mutation();

CREATE OR REPLACE FUNCTION ops.reject_published_composition_value_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    target_profile_id uuid;
BEGIN
    target_profile_id := CASE
        WHEN TG_OP = 'DELETE' THEN OLD.profile_id
        ELSE NEW.profile_id
    END;

    IF EXISTS (
        SELECT 1
          FROM composition.composition_profile profile
         WHERE profile.id = target_profile_id
           AND profile.status = 'published'
    ) THEN
        RAISE EXCEPTION 'published composition profile % values are immutable', target_profile_id;
    END IF;

    IF TG_OP = 'UPDATE'
       AND NEW.profile_id <> OLD.profile_id
       AND EXISTS (
           SELECT 1
             FROM composition.composition_profile profile
            WHERE profile.id = OLD.profile_id
              AND profile.status = 'published'
       ) THEN
        RAISE EXCEPTION 'published composition profile % values are immutable', OLD.profile_id;
    END IF;

    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE TRIGGER published_composition_value_immutable
    AFTER INSERT OR UPDATE OR DELETE ON composition.composition_value
    FOR EACH ROW EXECUTE FUNCTION ops.reject_published_composition_value_mutation();
