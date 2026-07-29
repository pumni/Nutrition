\set ON_ERROR_STOP on

BEGIN;

INSERT INTO catalog.food_entity (
    id, entity_kind, lifecycle_status, semantic_key
) VALUES (
    '0198f000-0000-7000-8000-000000000001',
    'basic_food',
    'active',
    'db_test_food'
);

INSERT INTO composition.nutrient (
    id, code, preferred_name, canonical_unit, nutrient_group
) VALUES (
    '0198f000-0000-7000-8000-000000000009',
    'db_test_nutrient',
    'Database test nutrient',
    'g',
    'test'
);

INSERT INTO recipe.recipe (
    id, output_food_id, recipe_type, lifecycle_status
) VALUES (
    '0198f000-0000-7000-8000-000000000002',
    '0198f000-0000-7000-8000-000000000001',
    'curated_baseline',
    'active'
);

INSERT INTO recipe.recipe_version (
    id, recipe_id, version_number, status, cooked_total_weight_g
) VALUES (
    '0198f000-0000-7000-8000-000000000003',
    '0198f000-0000-7000-8000-000000000002',
    1,
    'published',
    100
);

DO $$
DECLARE
    blocked boolean := false;
BEGIN
    BEGIN
        UPDATE recipe.recipe_version
           SET notes = 'must not change'
         WHERE id = '0198f000-0000-7000-8000-000000000003';
    EXCEPTION WHEN raise_exception THEN
        blocked := true;
    END;
    IF NOT blocked THEN
        RAISE EXCEPTION 'published recipe mutation was not blocked';
    END IF;
END;
$$;

INSERT INTO catalog.catalog_release (
    id, version, status, manifest, checksum_sha256, created_by
) VALUES (
    '0198f000-0000-7000-8000-000000000004',
    'db-test-release',
    'staged',
    '{}',
    repeat('a', 64),
    '0198f000-0000-7000-8000-000000000005'
);

INSERT INTO analysis.meal_analysis (
    id, locale, status
) VALUES (
    '0198f000-0000-7000-8000-000000000006',
    'vi-VN',
    'resolving'
);

INSERT INTO analysis.analysis_revision (
    id,
    meal_analysis_id,
    revision_number,
    revision_reason,
    application_version,
    parser_schema_version,
    prompt_version,
    model_provider_version,
    normalization_version,
    resolution_policy_version,
    portion_policy_version,
    composition_policy_version,
    calculation_engine_version,
    catalog_release_id,
    result_status,
    quality_label
) VALUES (
    '0198f000-0000-7000-8000-000000000007',
    '0198f000-0000-7000-8000-000000000006',
    1,
    'database verification',
    '0.1.0',
    'parsed-meal-0.1.0',
    'fixture-parser-0.1.0',
    'fixture/local',
    'normalize-0.1.0',
    'resolve-exact-0.1.0',
    'portion-explicit-0.1.0',
    'composition-direct-0.1.0',
    'calc-0.1.0',
    '0198f000-0000-7000-8000-000000000004',
    'building',
    'high'
);

INSERT INTO analysis.analysis_item (
    id,
    revision_id,
    item_index,
    source_text,
    parsed_payload,
    resolution_status,
    resolved_food_id,
    estimated_mass_g,
    evidence_quality
) VALUES (
    '0198f000-0000-7000-8000-000000000008',
    '0198f000-0000-7000-8000-000000000007',
    0,
    '100 g db test food',
    '{}',
    'resolved_exact',
    '0198f000-0000-7000-8000-000000000001',
    100,
    'A'
);

INSERT INTO analysis.resolution_candidate (
    item_id, rank, food_id, score, features
) VALUES (
    '0198f000-0000-7000-8000-000000000008',
    1,
    '0198f000-0000-7000-8000-000000000001',
    100,
    '{}'
);

INSERT INTO analysis.item_nutrient_result (
    item_id, nutrient_id, amount, unit, status
) VALUES (
    '0198f000-0000-7000-8000-000000000008',
    '0198f000-0000-7000-8000-000000000009',
    10,
    'g',
    'measured'
);

INSERT INTO analysis.revision_nutrient_total (
    revision_id, nutrient_id, amount, unit, completeness_ratio
) VALUES (
    '0198f000-0000-7000-8000-000000000007',
    '0198f000-0000-7000-8000-000000000009',
    10,
    'g',
    1
);

UPDATE analysis.analysis_revision
   SET result_status = 'completed',
       result_snapshot = '{}',
       snapshot_hash = repeat('a', 64)
 WHERE id = '0198f000-0000-7000-8000-000000000007';

DO $$
DECLARE
    revision_blocked boolean := false;
    child_blocked boolean := false;
    candidate_blocked boolean := false;
    item_result_blocked boolean := false;
    total_blocked boolean := false;
BEGIN
    BEGIN
        UPDATE analysis.analysis_revision
           SET quality_label = 'low'
         WHERE id = '0198f000-0000-7000-8000-000000000007';
    EXCEPTION WHEN raise_exception THEN
        revision_blocked := true;
    END;

    BEGIN
        UPDATE analysis.analysis_item
           SET estimated_mass_g = 200
         WHERE id = '0198f000-0000-7000-8000-000000000008';
    EXCEPTION WHEN raise_exception THEN
        child_blocked := true;
    END;

    BEGIN
        UPDATE analysis.resolution_candidate
           SET score = 99
         WHERE item_id = '0198f000-0000-7000-8000-000000000008'
           AND rank = 1;
    EXCEPTION WHEN raise_exception THEN
        candidate_blocked := true;
    END;

    BEGIN
        UPDATE analysis.item_nutrient_result
           SET amount = 20
         WHERE item_id = '0198f000-0000-7000-8000-000000000008'
           AND nutrient_id = '0198f000-0000-7000-8000-000000000009';
    EXCEPTION WHEN raise_exception THEN
        item_result_blocked := true;
    END;

    BEGIN
        UPDATE analysis.revision_nutrient_total
           SET amount = 20
         WHERE revision_id = '0198f000-0000-7000-8000-000000000007'
           AND nutrient_id = '0198f000-0000-7000-8000-000000000009';
    EXCEPTION WHEN raise_exception THEN
        total_blocked := true;
    END;

    IF NOT revision_blocked
       OR NOT child_blocked
       OR NOT candidate_blocked
       OR NOT item_result_blocked
       OR NOT total_blocked THEN
        RAISE EXCEPTION 'final analysis revision immutability was not enforced';
    END IF;
END;
$$;

ROLLBACK;
