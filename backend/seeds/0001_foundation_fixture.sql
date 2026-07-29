BEGIN;

INSERT INTO raw.dataset (
    id,
    code,
    name,
    publisher,
    license_code,
    license_url,
    homepage,
    ingestion_policy_version
) VALUES (
    '0198f100-0000-7000-8000-000000000010',
    'foundation_fixture',
    'Foundation engineering fixtures',
    'Nutrition backend engineering',
    'INTERNAL-TEST-ONLY',
    'fixture://terms/internal-test-only',
    'fixture://nutrition-backend',
    'ingest-foundation-0.1.0'
) ON CONFLICT DO NOTHING;

INSERT INTO raw.dataset_release (
    id,
    dataset_id,
    version,
    checksum_sha256,
    object_uri,
    schema_fingerprint,
    record_count,
    status,
    metadata
) VALUES (
    '0198f100-0000-7000-8000-000000000014',
    '0198f100-0000-7000-8000-000000000010',
    'foundation-0.2.0',
    repeat('6', 64),
    'fixture://foundation-0.2.0',
    repeat('7', 64),
    2,
    'imported',
    '{"production_eligible": false, "purpose": "contextual portion engineering verification"}'
) ON CONFLICT DO NOTHING;

INSERT INTO raw.dataset_release (
    id,
    dataset_id,
    version,
    checksum_sha256,
    object_uri,
    schema_fingerprint,
    record_count,
    status,
    metadata
) VALUES (
    '0198f100-0000-7000-8000-000000000011',
    '0198f100-0000-7000-8000-000000000010',
    'foundation-0.1.0',
    repeat('1', 64),
    'fixture://foundation-0.1.0',
    repeat('2', 64),
    2,
    'imported',
    '{"production_eligible": false, "purpose": "engineering verification"}'
) ON CONFLICT DO NOTHING;

INSERT INTO raw.source_food_record (
    id,
    dataset_release_id,
    external_id,
    source_data_type,
    source_description,
    normalized_search_text,
    raw_payload,
    payload_hash
) VALUES
(
    '0198f100-0000-7000-8000-000000000015',
    '0198f100-0000-7000-8000-000000000014',
    'fixture-boiled-egg-one-item',
    'portion_engineering_fixture',
    'Một quả trứng gà luộc',
    'một quả trứng gà luộc',
    '{"test_only": true, "measure": "quả", "central_g": 50, "lower_g": 45, "upper_g": 60}',
    repeat('8', 64)
),
(
    '0198f100-0000-7000-8000-000000000016',
    '0198f100-0000-7000-8000-000000000014',
    'fixture-white-rice-one-bowl',
    'portion_engineering_fixture',
    'Một bát cơm trắng',
    'một bát cơm trắng',
    '{"test_only": true, "measure": "bát", "central_g": 150, "lower_g": 120, "upper_g": 200}',
    repeat('9', 64)
) ON CONFLICT DO NOTHING;

INSERT INTO raw.source_food_record (
    id,
    dataset_release_id,
    external_id,
    source_data_type,
    source_description,
    normalized_search_text,
    raw_payload,
    payload_hash
) VALUES
(
    '0198f100-0000-7000-8000-000000000012',
    '0198f100-0000-7000-8000-000000000011',
    'fixture-boiled-egg',
    'engineering_fixture',
    'Trứng gà luộc',
    'trứng gà luộc',
    '{"test_only": true, "name": "Trứng gà luộc"}',
    repeat('3', 64)
),
(
    '0198f100-0000-7000-8000-000000000013',
    '0198f100-0000-7000-8000-000000000011',
    'fixture-white-rice',
    'engineering_fixture',
    'Cơm trắng',
    'cơm trắng',
    '{"test_only": true, "name": "Cơm trắng"}',
    repeat('4', 64)
) ON CONFLICT DO NOTHING;

INSERT INTO raw.source_activation (
    dataset_id,
    active_release_id,
    activated_by,
    reason
) VALUES (
    '0198f100-0000-7000-8000-000000000010',
    '0198f100-0000-7000-8000-000000000011',
    '0198f100-0000-7000-8000-000000000099',
    'Local integration-test seed'
) ON CONFLICT DO NOTHING;

INSERT INTO catalog.food_entity (
    id,
    entity_kind,
    lifecycle_status,
    semantic_key
) VALUES
(
    '0198f100-0000-7000-8000-000000000020',
    'basic_food',
    'active',
    'fixture_boiled_egg'
),
(
    '0198f100-0000-7000-8000-000000000021',
    'processed_food',
    'active',
    'fixture_white_rice'
) ON CONFLICT DO NOTHING;

INSERT INTO catalog.food_name (
    id,
    food_id,
    locale,
    name,
    normalized_name,
    normalized_name_no_diacritics,
    name_type,
    is_curated,
    source_record_id,
    search_weight
) VALUES
(
    '0198f100-0000-7000-8000-000000000022',
    '0198f100-0000-7000-8000-000000000020',
    'vi-VN',
    'Trứng gà luộc',
    'trứng gà luộc',
    'trung ga luoc',
    'preferred',
    true,
    '0198f100-0000-7000-8000-000000000012',
    100
),
(
    '0198f100-0000-7000-8000-000000000023',
    '0198f100-0000-7000-8000-000000000021',
    'vi-VN',
    'Cơm trắng',
    'cơm trắng',
    'com trang',
    'preferred',
    true,
    '0198f100-0000-7000-8000-000000000013',
    100
) ON CONFLICT DO NOTHING;

INSERT INTO catalog.food_mapping (
    id,
    source_food_record_id,
    food_id,
    mapping_type,
    mapping_method,
    score,
    policy_version,
    review_status,
    reviewed_by,
    reviewed_at,
    rationale
) VALUES
(
    '0198f100-0000-7000-8000-000000000024',
    '0198f100-0000-7000-8000-000000000012',
    '0198f100-0000-7000-8000-000000000020',
    'exact',
    'fixture_manual_review',
    100,
    'mapping-foundation-0.1.0',
    'approved',
    '0198f100-0000-7000-8000-000000000099',
    now(),
    'Engineering fixture only'
),
(
    '0198f100-0000-7000-8000-000000000025',
    '0198f100-0000-7000-8000-000000000013',
    '0198f100-0000-7000-8000-000000000021',
    'exact',
    'fixture_manual_review',
    100,
    'mapping-foundation-0.1.0',
    'approved',
    '0198f100-0000-7000-8000-000000000099',
    now(),
    'Engineering fixture only'
) ON CONFLICT DO NOTHING;

INSERT INTO composition.nutrient (
    id,
    code,
    preferred_name,
    canonical_unit,
    nutrient_group,
    is_energy_component
) VALUES
(
    '0198f100-0000-7000-8000-000000000030',
    'energy_kcal',
    'Energy',
    'kcal',
    'energy',
    false
),
(
    '0198f100-0000-7000-8000-000000000031',
    'protein_g',
    'Protein',
    'g',
    'macronutrient',
    true
),
(
    '0198f100-0000-7000-8000-000000000032',
    'carbohydrate_g',
    'Carbohydrate',
    'g',
    'macronutrient',
    true
),
(
    '0198f100-0000-7000-8000-000000000033',
    'fat_g',
    'Fat',
    'g',
    'macronutrient',
    true
) ON CONFLICT DO NOTHING;

INSERT INTO composition.composition_profile (
    id,
    food_id,
    profile_type,
    basis_amount,
    basis_unit,
    edible_basis,
    source_record_id,
    quality_grade,
    status,
    method_metadata
) VALUES
(
    '0198f100-0000-7000-8000-000000000040',
    '0198f100-0000-7000-8000-000000000020',
    'laboratory',
    100,
    'g',
    true,
    '0198f100-0000-7000-8000-000000000012',
    'A',
    'in_review',
    '{"test_only": true}'
),
(
    '0198f100-0000-7000-8000-000000000041',
    '0198f100-0000-7000-8000-000000000021',
    'laboratory',
    100,
    'g',
    true,
    '0198f100-0000-7000-8000-000000000013',
    'A',
    'in_review',
    '{"test_only": true}'
) ON CONFLICT DO NOTHING;

INSERT INTO composition.measure_unit (
    id,
    code,
    dimension,
    canonical_label_vi,
    aliases
) VALUES
(
    '0198f100-0000-7000-8000-000000000060',
    'g',
    'mass',
    'g',
    '["gram", "grams"]'
),
(
    '0198f100-0000-7000-8000-000000000061',
    'qua',
    'count',
    'quả',
    '["trái"]'
),
(
    '0198f100-0000-7000-8000-000000000062',
    'bat',
    'household',
    'bát',
    '["chén"]'
) ON CONFLICT DO NOTHING;

INSERT INTO composition.portion_observation (
    id,
    food_id,
    measure_unit_id,
    measure_amount,
    gram_weight,
    lower_bound_g,
    upper_bound_g,
    region_code,
    context_type,
    source_record_id,
    estimation_method,
    quality_grade,
    metadata
) VALUES
(
    '0198f100-0000-7000-8000-000000000050',
    '0198f100-0000-7000-8000-000000000020',
    '0198f100-0000-7000-8000-000000000061',
    1,
    50,
    45,
    60,
    'VN',
    'cooked_whole_item',
    '0198f100-0000-7000-8000-000000000015',
    'engineering_fixture',
    'C',
    '{"test_only": true}'
),
(
    '0198f100-0000-7000-8000-000000000051',
    '0198f100-0000-7000-8000-000000000021',
    '0198f100-0000-7000-8000-000000000062',
    1,
    150,
    120,
    200,
    'VN',
    'cooked_bowl',
    '0198f100-0000-7000-8000-000000000016',
    'engineering_fixture',
    'C',
    '{"test_only": true}'
) ON CONFLICT DO NOTHING;

INSERT INTO composition.composition_value (
    profile_id,
    nutrient_id,
    amount,
    canonical_amount,
    unit,
    value_status
) VALUES
('0198f100-0000-7000-8000-000000000040', '0198f100-0000-7000-8000-000000000030', 155, 155, 'kcal', 'measured'),
('0198f100-0000-7000-8000-000000000040', '0198f100-0000-7000-8000-000000000031', 12.6, 12.6, 'g', 'measured'),
('0198f100-0000-7000-8000-000000000040', '0198f100-0000-7000-8000-000000000032', 1.12, 1.12, 'g', 'measured'),
('0198f100-0000-7000-8000-000000000040', '0198f100-0000-7000-8000-000000000033', 10.6, 10.6, 'g', 'measured'),
('0198f100-0000-7000-8000-000000000041', '0198f100-0000-7000-8000-000000000030', 130, 130, 'kcal', 'measured'),
('0198f100-0000-7000-8000-000000000041', '0198f100-0000-7000-8000-000000000031', 2.69, 2.69, 'g', 'measured'),
('0198f100-0000-7000-8000-000000000041', '0198f100-0000-7000-8000-000000000032', 28.2, 28.2, 'g', 'measured'),
('0198f100-0000-7000-8000-000000000041', '0198f100-0000-7000-8000-000000000033', 0.28, 0.28, 'g', 'measured')
ON CONFLICT DO NOTHING;

UPDATE composition.composition_profile
   SET status = 'published'
 WHERE id IN (
    '0198f100-0000-7000-8000-000000000040',
    '0198f100-0000-7000-8000-000000000041'
 )
   AND status = 'in_review';

INSERT INTO catalog.catalog_release (
    id,
    version,
    status,
    manifest,
    checksum_sha256,
    created_by
) VALUES (
    '0198f100-0000-7000-8000-000000000001',
    'catalog-foundation-0.1.0',
    'staged',
    '{
      "production_eligible": false,
      "dataset_releases": ["0198f100-0000-7000-8000-000000000011"],
      "foods": 2,
      "profiles": 2
    }',
    repeat('5', 64),
    '0198f100-0000-7000-8000-000000000099'
) ON CONFLICT DO NOTHING;

INSERT INTO catalog.catalog_release_food_name (
    catalog_release_id,
    food_name_id
) VALUES
(
    '0198f100-0000-7000-8000-000000000001',
    '0198f100-0000-7000-8000-000000000022'
),
(
    '0198f100-0000-7000-8000-000000000001',
    '0198f100-0000-7000-8000-000000000023'
) ON CONFLICT DO NOTHING;

INSERT INTO catalog.catalog_release_profile (
    catalog_release_id,
    profile_id
) VALUES
(
    '0198f100-0000-7000-8000-000000000001',
    '0198f100-0000-7000-8000-000000000040'
),
(
    '0198f100-0000-7000-8000-000000000001',
    '0198f100-0000-7000-8000-000000000041'
) ON CONFLICT DO NOTHING;

INSERT INTO catalog.catalog_release (
    id,
    version,
    status,
    manifest,
    checksum_sha256,
    created_by
) VALUES (
    '0198f100-0000-7000-8000-000000000002',
    'catalog-foundation-0.2.0',
    'staged',
    '{
      "production_eligible": false,
      "dataset_releases": [
        "0198f100-0000-7000-8000-000000000011",
        "0198f100-0000-7000-8000-000000000014"
      ],
      "foods": 2,
      "profiles": 2,
      "portion_observations": 2
    }',
    repeat('a', 64),
    '0198f100-0000-7000-8000-000000000099'
) ON CONFLICT DO NOTHING;

INSERT INTO catalog.catalog_release_food_name (
    catalog_release_id,
    food_name_id
) VALUES
(
    '0198f100-0000-7000-8000-000000000002',
    '0198f100-0000-7000-8000-000000000022'
),
(
    '0198f100-0000-7000-8000-000000000002',
    '0198f100-0000-7000-8000-000000000023'
) ON CONFLICT DO NOTHING;

INSERT INTO catalog.catalog_release_profile (
    catalog_release_id,
    profile_id
) VALUES
(
    '0198f100-0000-7000-8000-000000000002',
    '0198f100-0000-7000-8000-000000000040'
),
(
    '0198f100-0000-7000-8000-000000000002',
    '0198f100-0000-7000-8000-000000000041'
) ON CONFLICT DO NOTHING;

INSERT INTO catalog.catalog_release_portion_observation (
    catalog_release_id,
    portion_observation_id
) VALUES
(
    '0198f100-0000-7000-8000-000000000002',
    '0198f100-0000-7000-8000-000000000050'
),
(
    '0198f100-0000-7000-8000-000000000002',
    '0198f100-0000-7000-8000-000000000051'
) ON CONFLICT DO NOTHING;

UPDATE raw.source_activation
   SET previous_release_id = active_release_id,
       active_release_id = '0198f100-0000-7000-8000-000000000014',
       activated_by = '0198f100-0000-7000-8000-000000000099',
       activated_at = now(),
       reason = 'Contextual portion integration-test seed'
 WHERE dataset_id = '0198f100-0000-7000-8000-000000000010'
   AND active_release_id <> '0198f100-0000-7000-8000-000000000014';

UPDATE catalog.catalog_release
   SET status = 'superseded'
 WHERE status = 'active'
   AND id <> '0198f100-0000-7000-8000-000000000002';

UPDATE catalog.catalog_release
   SET status = 'superseded',
       activated_at = COALESCE(activated_at, now())
 WHERE id = '0198f100-0000-7000-8000-000000000001'
   AND status = 'staged';

UPDATE catalog.catalog_release
   SET status = 'active',
       activated_at = COALESCE(activated_at, now())
 WHERE id = '0198f100-0000-7000-8000-000000000002'
   AND status = 'staged';

COMMIT;
