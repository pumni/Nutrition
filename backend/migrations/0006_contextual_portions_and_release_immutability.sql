CREATE TABLE catalog.catalog_release_portion_observation (
    catalog_release_id uuid NOT NULL REFERENCES catalog.catalog_release(id),
    portion_observation_id uuid NOT NULL REFERENCES composition.portion_observation(id),
    PRIMARY KEY (catalog_release_id, portion_observation_id)
);

CREATE OR REPLACE FUNCTION ops.reject_published_catalog_membership_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    target_release_id uuid;
    target_status text;
BEGIN
    IF TG_OP = 'INSERT' THEN
        target_release_id := NEW.catalog_release_id;
    ELSE
        target_release_id := OLD.catalog_release_id;
    END IF;

    SELECT release.status
      INTO target_status
      FROM catalog.catalog_release release
     WHERE release.id = target_release_id;

    IF target_status IN ('active', 'superseded') THEN
        RAISE EXCEPTION 'published catalog release % memberships are immutable', target_release_id;
    END IF;

    IF TG_OP = 'UPDATE' AND NEW.catalog_release_id <> OLD.catalog_release_id THEN
        SELECT release.status
          INTO target_status
          FROM catalog.catalog_release release
         WHERE release.id = NEW.catalog_release_id;

        IF target_status IN ('active', 'superseded') THEN
            RAISE EXCEPTION 'published catalog release % memberships are immutable',
                NEW.catalog_release_id;
        END IF;
    END IF;

    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE TRIGGER catalog_release_food_name_membership_immutable
    AFTER INSERT OR UPDATE OR DELETE ON catalog.catalog_release_food_name
    FOR EACH ROW EXECUTE FUNCTION ops.reject_published_catalog_membership_mutation();

CREATE TRIGGER catalog_release_profile_membership_immutable
    AFTER INSERT OR UPDATE OR DELETE ON catalog.catalog_release_profile
    FOR EACH ROW EXECUTE FUNCTION ops.reject_published_catalog_membership_mutation();

CREATE TRIGGER catalog_release_portion_membership_immutable
    AFTER INSERT OR UPDATE OR DELETE ON catalog.catalog_release_portion_observation
    FOR EACH ROW EXECUTE FUNCTION ops.reject_published_catalog_membership_mutation();

CREATE OR REPLACE FUNCTION ops.reject_published_catalog_release_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF TG_OP = 'DELETE' AND OLD.status IN ('active', 'superseded') THEN
        RAISE EXCEPTION 'published catalog release % is immutable', OLD.id;
    END IF;

    IF TG_OP = 'UPDATE' AND OLD.status IN ('active', 'superseded') THEN
        IF OLD.status = 'active'
           AND NEW.status = 'superseded'
           AND NEW.id = OLD.id
           AND NEW.version = OLD.version
           AND NEW.manifest = OLD.manifest
           AND NEW.checksum_sha256 = OLD.checksum_sha256
           AND NEW.created_by = OLD.created_by
           AND NEW.created_at = OLD.created_at
           AND NEW.activated_at IS NOT DISTINCT FROM OLD.activated_at THEN
            RETURN NEW;
        END IF;

        RAISE EXCEPTION 'published catalog release % is immutable', OLD.id;
    END IF;

    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE TRIGGER published_catalog_release_immutable
    BEFORE UPDATE OR DELETE ON catalog.catalog_release
    FOR EACH ROW EXECUTE FUNCTION ops.reject_published_catalog_release_mutation();
