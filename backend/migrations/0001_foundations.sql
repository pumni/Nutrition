CREATE EXTENSION IF NOT EXISTS pg_trgm;

CREATE SCHEMA raw;
CREATE SCHEMA catalog;
CREATE SCHEMA recipe;
CREATE SCHEMA composition;
CREATE SCHEMA analysis;
CREATE SCHEMA app;
CREATE SCHEMA ops;

CREATE TABLE raw.dataset (
    id uuid PRIMARY KEY,
    code text NOT NULL UNIQUE,
    name text NOT NULL,
    publisher text NOT NULL,
    license_code text,
    license_url text,
    homepage text,
    ingestion_policy_version text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE raw.dataset_release (
    id uuid PRIMARY KEY,
    dataset_id uuid NOT NULL REFERENCES raw.dataset(id),
    version text NOT NULL,
    published_at timestamptz,
    imported_at timestamptz NOT NULL DEFAULT now(),
    checksum_sha256 text NOT NULL CHECK (checksum_sha256 ~ '^[0-9a-f]{64}$'),
    object_uri text NOT NULL,
    schema_fingerprint text NOT NULL,
    record_count bigint CHECK (record_count IS NULL OR record_count >= 0),
    status text NOT NULL CHECK (status IN (
        'received', 'validated', 'imported', 'failed', 'superseded'
    )),
    metadata jsonb NOT NULL DEFAULT '{}',
    UNIQUE (dataset_id, version),
    UNIQUE (dataset_id, checksum_sha256)
);

CREATE TABLE raw.source_food_record (
    id uuid PRIMARY KEY,
    dataset_release_id uuid NOT NULL REFERENCES raw.dataset_release(id),
    external_id text NOT NULL,
    source_data_type text,
    source_description text NOT NULL,
    normalized_search_text text,
    raw_payload jsonb NOT NULL,
    payload_hash text NOT NULL CHECK (payload_hash ~ '^[0-9a-f]{64}$'),
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (dataset_release_id, external_id)
);

CREATE TABLE raw.source_activation (
    dataset_id uuid PRIMARY KEY REFERENCES raw.dataset(id),
    active_release_id uuid NOT NULL REFERENCES raw.dataset_release(id),
    previous_release_id uuid REFERENCES raw.dataset_release(id),
    activated_by uuid NOT NULL,
    activated_at timestamptz NOT NULL DEFAULT now(),
    reason text NOT NULL
);

CREATE TABLE catalog.food_entity (
    id uuid PRIMARY KEY,
    entity_kind text NOT NULL CHECK (entity_kind IN (
        'basic_food', 'processed_food', 'dish', 'branded_product'
    )),
    lifecycle_status text NOT NULL CHECK (lifecycle_status IN (
        'draft', 'active', 'deprecated', 'merged', 'rejected'
    )),
    replacement_food_id uuid REFERENCES catalog.food_entity(id),
    semantic_key text UNIQUE,
    created_by uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    CHECK (
        (lifecycle_status IN ('deprecated', 'merged') AND replacement_food_id IS NOT NULL)
        OR lifecycle_status NOT IN ('deprecated', 'merged')
    ),
    CHECK (replacement_food_id IS NULL OR replacement_food_id <> id)
);

CREATE TABLE catalog.food_name (
    id uuid PRIMARY KEY,
    food_id uuid NOT NULL REFERENCES catalog.food_entity(id),
    locale text NOT NULL,
    region_code text,
    name text NOT NULL,
    normalized_name text NOT NULL,
    normalized_name_no_diacritics text,
    name_type text NOT NULL CHECK (name_type IN (
        'preferred', 'alias', 'colloquial', 'brand', 'misspelling', 'transliteration'
    )),
    is_curated boolean NOT NULL DEFAULT false,
    source_record_id uuid REFERENCES raw.source_food_record(id),
    valid_from timestamptz NOT NULL DEFAULT now(),
    valid_to timestamptz,
    search_weight smallint NOT NULL DEFAULT 0,
    CHECK (valid_to IS NULL OR valid_to > valid_from)
);

CREATE UNIQUE INDEX uq_food_preferred_name_scope
    ON catalog.food_name (food_id, locale, (COALESCE(region_code, '')))
    WHERE name_type = 'preferred' AND valid_to IS NULL;
CREATE INDEX ix_food_name_normalized
    ON catalog.food_name (normalized_name)
    WHERE valid_to IS NULL;
CREATE INDEX ix_food_name_no_diacritics
    ON catalog.food_name (normalized_name_no_diacritics)
    WHERE valid_to IS NULL AND normalized_name_no_diacritics IS NOT NULL;
CREATE INDEX ix_food_name_trgm
    ON catalog.food_name USING gin (normalized_name gin_trgm_ops)
    WHERE valid_to IS NULL;

CREATE TABLE catalog.food_mapping (
    id uuid PRIMARY KEY,
    source_food_record_id uuid NOT NULL REFERENCES raw.source_food_record(id),
    food_id uuid NOT NULL REFERENCES catalog.food_entity(id),
    mapping_type text NOT NULL CHECK (mapping_type IN (
        'exact', 'broader', 'narrower', 'approximate', 'rejected'
    )),
    mapping_method text NOT NULL,
    score double precision CHECK (score IS NULL OR score BETWEEN -1000000 AND 1000000),
    policy_version text NOT NULL,
    review_status text NOT NULL CHECK (review_status IN (
        'proposed', 'approved', 'rejected', 'superseded'
    )),
    reviewed_by uuid,
    reviewed_at timestamptz,
    rationale text,
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (review_status <> 'approved' OR reviewed_by IS NOT NULL)
);

CREATE UNIQUE INDEX uq_current_approved_source_mapping
    ON catalog.food_mapping (source_food_record_id)
    WHERE review_status = 'approved' AND mapping_type <> 'rejected';

CREATE TABLE catalog.catalog_release (
    id uuid PRIMARY KEY,
    version text NOT NULL UNIQUE,
    status text NOT NULL CHECK (status IN ('draft', 'staged', 'active', 'superseded')),
    manifest jsonb NOT NULL,
    checksum_sha256 text NOT NULL CHECK (checksum_sha256 ~ '^[0-9a-f]{64}$'),
    created_by uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    activated_at timestamptz
);

CREATE TABLE composition.nutrient (
    id uuid PRIMARY KEY,
    code text NOT NULL UNIQUE,
    preferred_name text NOT NULL,
    canonical_unit text NOT NULL CHECK (canonical_unit IN ('kcal', 'g', 'mg', 'ug')),
    nutrient_group text NOT NULL,
    external_identifiers jsonb NOT NULL DEFAULT '{}',
    is_energy_component boolean NOT NULL DEFAULT false
);

CREATE TABLE composition.measure_unit (
    id uuid PRIMARY KEY,
    code text NOT NULL UNIQUE,
    dimension text NOT NULL CHECK (dimension IN (
        'mass', 'volume', 'count', 'household', 'serving'
    )),
    canonical_label_vi text NOT NULL,
    aliases jsonb NOT NULL DEFAULT '[]'
);

CREATE TABLE recipe.recipe (
    id uuid PRIMARY KEY,
    output_food_id uuid NOT NULL REFERENCES catalog.food_entity(id),
    recipe_type text NOT NULL CHECK (recipe_type IN (
        'curated_baseline', 'regional_variant', 'manufacturer', 'user_defined'
    )),
    region_code text,
    owner_user_id uuid,
    lifecycle_status text NOT NULL CHECK (lifecycle_status IN (
        'draft', 'active', 'deprecated', 'rejected'
    )),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE recipe.recipe_version (
    id uuid PRIMARY KEY,
    recipe_id uuid NOT NULL REFERENCES recipe.recipe(id),
    version_number integer NOT NULL CHECK (version_number > 0),
    status text NOT NULL CHECK (status IN (
        'draft', 'in_review', 'published', 'deprecated', 'rejected'
    )),
    raw_total_weight_g numeric(14,4),
    cooked_total_weight_g numeric(14,4),
    serving_count numeric(12,4),
    declared_yield_factor numeric(10,6),
    source_record_id uuid REFERENCES raw.source_food_record(id),
    notes text,
    created_by uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    published_at timestamptz,
    UNIQUE (recipe_id, version_number),
    CHECK (raw_total_weight_g IS NULL OR raw_total_weight_g > 0),
    CHECK (cooked_total_weight_g IS NULL OR cooked_total_weight_g > 0),
    CHECK (serving_count IS NULL OR serving_count > 0),
    CHECK (declared_yield_factor IS NULL OR declared_yield_factor > 0)
);

CREATE TABLE recipe.recipe_component (
    id uuid PRIMARY KEY,
    recipe_version_id uuid NOT NULL REFERENCES recipe.recipe_version(id),
    component_food_id uuid NOT NULL REFERENCES catalog.food_entity(id),
    component_role text NOT NULL DEFAULT 'ingredient',
    sequence_number integer NOT NULL CHECK (sequence_number > 0),
    quantity numeric(14,6) NOT NULL CHECK (quantity > 0),
    unit_id uuid NOT NULL REFERENCES composition.measure_unit(id),
    resolved_weight_g numeric(14,4),
    edible_fraction numeric(8,6),
    preparation_method_code text,
    is_optional boolean NOT NULL DEFAULT false,
    notes text,
    UNIQUE (recipe_version_id, sequence_number),
    CHECK (resolved_weight_g IS NULL OR resolved_weight_g > 0),
    CHECK (edible_fraction IS NULL OR edible_fraction > 0 AND edible_fraction <= 1)
);

CREATE TABLE composition.composition_profile (
    id uuid PRIMARY KEY,
    food_id uuid NOT NULL REFERENCES catalog.food_entity(id),
    profile_type text NOT NULL CHECK (profile_type IN (
        'laboratory', 'declared_label', 'recipe_calculated', 'compiled', 'imputed'
    )),
    basis_amount numeric(14,6) NOT NULL CHECK (basis_amount > 0),
    basis_unit text NOT NULL CHECK (basis_unit IN ('g', 'ml', 'serving', 'package')),
    edible_basis boolean NOT NULL,
    source_record_id uuid REFERENCES raw.source_food_record(id),
    recipe_version_id uuid REFERENCES recipe.recipe_version(id),
    calculation_run_id uuid,
    quality_grade text NOT NULL CHECK (quality_grade IN ('A', 'B', 'C', 'D', 'U')),
    status text NOT NULL CHECK (status IN (
        'draft', 'in_review', 'published', 'deprecated', 'rejected'
    )),
    valid_from timestamptz,
    valid_to timestamptz,
    method_metadata jsonb NOT NULL DEFAULT '{}',
    created_at timestamptz NOT NULL DEFAULT now(),
    CHECK (valid_to IS NULL OR valid_from IS NULL OR valid_to > valid_from),
    CHECK (
        profile_type <> 'recipe_calculated'
        OR (recipe_version_id IS NOT NULL AND calculation_run_id IS NOT NULL)
    )
);

CREATE TABLE composition.composition_value (
    profile_id uuid NOT NULL REFERENCES composition.composition_profile(id),
    nutrient_id uuid NOT NULL REFERENCES composition.nutrient(id),
    amount numeric(18,8),
    canonical_amount numeric(18,8),
    unit text NOT NULL,
    minimum_amount numeric(18,8),
    maximum_amount numeric(18,8),
    value_status text NOT NULL CHECK (value_status IN (
        'measured', 'declared', 'calculated', 'compiled', 'estimated',
        'trace', 'not_detected', 'missing'
    )),
    significant_figures smallint,
    method_code text,
    PRIMARY KEY (profile_id, nutrient_id),
    CHECK (amount IS NULL OR amount >= 0),
    CHECK (canonical_amount IS NULL OR canonical_amount >= 0),
    CHECK (minimum_amount IS NULL OR minimum_amount >= 0),
    CHECK (maximum_amount IS NULL OR maximum_amount >= 0),
    CHECK ((value_status = 'missing' AND amount IS NULL) OR value_status <> 'missing'),
    CHECK (minimum_amount IS NULL OR maximum_amount IS NULL OR minimum_amount <= maximum_amount)
);

CREATE TABLE composition.portion_observation (
    id uuid PRIMARY KEY,
    food_id uuid NOT NULL REFERENCES catalog.food_entity(id),
    measure_unit_id uuid NOT NULL REFERENCES composition.measure_unit(id),
    measure_amount numeric(14,6) NOT NULL CHECK (measure_amount > 0),
    gram_weight numeric(14,4) NOT NULL CHECK (gram_weight > 0),
    lower_bound_g numeric(14,4),
    upper_bound_g numeric(14,4),
    region_code text,
    context_type text,
    source_record_id uuid REFERENCES raw.source_food_record(id),
    estimation_method text NOT NULL,
    quality_grade text NOT NULL CHECK (quality_grade IN ('A', 'B', 'C', 'D', 'U')),
    sample_count integer CHECK (sample_count IS NULL OR sample_count > 0),
    valid_from timestamptz,
    valid_to timestamptz,
    metadata jsonb NOT NULL DEFAULT '{}',
    CHECK (lower_bound_g IS NULL OR lower_bound_g > 0),
    CHECK (upper_bound_g IS NULL OR upper_bound_g > 0),
    CHECK (lower_bound_g IS NULL OR upper_bound_g IS NULL OR lower_bound_g <= upper_bound_g),
    CHECK (valid_to IS NULL OR valid_from IS NULL OR valid_to > valid_from)
);

CREATE TABLE analysis.meal_analysis (
    id uuid PRIMARY KEY,
    user_id uuid,
    raw_text_ciphertext bytea,
    locale text NOT NULL,
    occurred_at timestamptz,
    idempotency_key text,
    status text NOT NULL CHECK (status IN (
        'received', 'parsing', 'resolving', 'needs_clarification',
        'completed', 'insufficient_evidence', 'confirmed', 'corrected', 'abandoned'
    )),
    current_revision_id uuid,
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE NULLS NOT DISTINCT (user_id, idempotency_key)
);

CREATE TABLE analysis.analysis_revision (
    id uuid PRIMARY KEY,
    meal_analysis_id uuid NOT NULL REFERENCES analysis.meal_analysis(id),
    revision_number integer NOT NULL CHECK (revision_number > 0),
    revision_reason text NOT NULL,
    application_version text NOT NULL,
    parser_schema_version text NOT NULL,
    prompt_version text NOT NULL,
    model_provider_version text NOT NULL,
    normalization_version text NOT NULL,
    resolution_policy_version text NOT NULL,
    portion_policy_version text NOT NULL,
    composition_policy_version text NOT NULL,
    calculation_engine_version text NOT NULL,
    catalog_release_id uuid NOT NULL REFERENCES catalog.catalog_release(id),
    result_status text NOT NULL CHECK (result_status IN (
        'building', 'needs_clarification', 'completed', 'insufficient_evidence'
    )),
    quality_label text NOT NULL CHECK (quality_label IN (
        'high', 'medium', 'low', 'insufficient'
    )),
    assumptions jsonb NOT NULL DEFAULT '[]',
    warnings jsonb NOT NULL DEFAULT '[]',
    created_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (meal_analysis_id, revision_number)
);

ALTER TABLE analysis.meal_analysis
    ADD CONSTRAINT meal_analysis_current_revision_fk
    FOREIGN KEY (current_revision_id) REFERENCES analysis.analysis_revision(id);

CREATE TABLE analysis.analysis_item (
    id uuid PRIMARY KEY,
    revision_id uuid NOT NULL REFERENCES analysis.analysis_revision(id),
    item_index integer NOT NULL CHECK (item_index >= 0),
    source_text text NOT NULL,
    parsed_payload jsonb NOT NULL,
    resolution_status text NOT NULL CHECK (resolution_status IN (
        'resolved_exact', 'resolved_high_evidence', 'resolved_with_assumption',
        'needs_clarification', 'unresolved'
    )),
    resolved_food_id uuid REFERENCES catalog.food_entity(id),
    resolved_recipe_version_id uuid REFERENCES recipe.recipe_version(id),
    resolved_profile_id uuid REFERENCES composition.composition_profile(id),
    resolved_portion_observation_id uuid REFERENCES composition.portion_observation(id),
    estimated_mass_g numeric(14,4),
    lower_mass_g numeric(14,4),
    upper_mass_g numeric(14,4),
    evidence_quality text NOT NULL CHECK (evidence_quality IN ('A', 'B', 'C', 'D', 'U')),
    UNIQUE (revision_id, item_index),
    CHECK (
        (resolved_food_id IS NOT NULL AND resolution_status LIKE 'resolved%')
        OR (resolved_food_id IS NULL AND resolution_status IN ('needs_clarification', 'unresolved'))
    ),
    CHECK (estimated_mass_g IS NULL OR estimated_mass_g > 0),
    CHECK (lower_mass_g IS NULL OR lower_mass_g > 0),
    CHECK (upper_mass_g IS NULL OR upper_mass_g > 0),
    CHECK (lower_mass_g IS NULL OR estimated_mass_g IS NULL OR lower_mass_g <= estimated_mass_g),
    CHECK (upper_mass_g IS NULL OR estimated_mass_g IS NULL OR upper_mass_g >= estimated_mass_g)
);

CREATE TABLE analysis.resolution_candidate (
    item_id uuid NOT NULL REFERENCES analysis.analysis_item(id),
    rank integer NOT NULL CHECK (rank > 0),
    food_id uuid NOT NULL REFERENCES catalog.food_entity(id),
    score double precision NOT NULL CHECK (score BETWEEN -1000000 AND 1000000),
    features jsonb NOT NULL,
    decision text,
    PRIMARY KEY (item_id, rank)
);

CREATE TABLE analysis.item_nutrient_result (
    item_id uuid NOT NULL REFERENCES analysis.analysis_item(id),
    nutrient_id uuid NOT NULL REFERENCES composition.nutrient(id),
    amount numeric(18,8),
    lower_amount numeric(18,8),
    upper_amount numeric(18,8),
    unit text NOT NULL,
    status text NOT NULL,
    calculation_trace jsonb,
    PRIMARY KEY (item_id, nutrient_id),
    CHECK (amount IS NULL OR amount >= 0),
    CHECK (lower_amount IS NULL OR lower_amount >= 0),
    CHECK (upper_amount IS NULL OR upper_amount >= 0)
);

CREATE TABLE analysis.revision_nutrient_total (
    revision_id uuid NOT NULL REFERENCES analysis.analysis_revision(id),
    nutrient_id uuid NOT NULL REFERENCES composition.nutrient(id),
    amount numeric(18,8),
    lower_amount numeric(18,8),
    upper_amount numeric(18,8),
    unit text NOT NULL,
    completeness_ratio numeric(8,6) NOT NULL CHECK (
        completeness_ratio >= 0 AND completeness_ratio <= 1
    ),
    PRIMARY KEY (revision_id, nutrient_id)
);

CREATE TABLE analysis.clarification_question (
    id uuid PRIMARY KEY,
    analysis_revision_id uuid NOT NULL REFERENCES analysis.analysis_revision(id),
    dimension text NOT NULL,
    prompt text NOT NULL,
    options jsonb NOT NULL,
    policy_version text NOT NULL,
    status text NOT NULL CHECK (status IN ('open', 'answered', 'expired', 'cancelled')),
    created_at timestamptz NOT NULL DEFAULT now(),
    answered_at timestamptz
);

CREATE UNIQUE INDEX clarification_one_open_per_revision
    ON analysis.clarification_question (analysis_revision_id)
    WHERE status = 'open';

CREATE TABLE analysis.clarification_answer (
    id uuid PRIMARY KEY,
    question_id uuid NOT NULL REFERENCES analysis.clarification_question(id),
    expected_revision_id uuid NOT NULL REFERENCES analysis.analysis_revision(id),
    option_id text,
    free_text_ciphertext bytea,
    created_revision_id uuid REFERENCES analysis.analysis_revision(id),
    answered_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (question_id)
);

CREATE TABLE app.analysis_correction (
    id uuid PRIMARY KEY,
    meal_analysis_id uuid NOT NULL REFERENCES analysis.meal_analysis(id),
    base_revision_id uuid NOT NULL REFERENCES analysis.analysis_revision(id),
    actor_type text NOT NULL CHECK (actor_type IN ('user', 'curator', 'system_migration')),
    actor_id uuid,
    correction_payload jsonb NOT NULL,
    created_revision_id uuid REFERENCES analysis.analysis_revision(id),
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE app.idempotency_record (
    scope_key text NOT NULL,
    idempotency_key text NOT NULL,
    request_hash text NOT NULL CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    response_reference jsonb,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (scope_key, idempotency_key)
);

CREATE TABLE ops.job (
    id uuid PRIMARY KEY,
    job_type text NOT NULL,
    payload jsonb NOT NULL,
    status text NOT NULL CHECK (status IN ('queued', 'running', 'retry', 'completed', 'dead')),
    attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts integer NOT NULL DEFAULT 5 CHECK (max_attempts > 0),
    available_at timestamptz NOT NULL DEFAULT now(),
    locked_at timestamptz,
    locked_by text,
    last_error text,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX ix_job_claim
    ON ops.job (status, available_at)
    WHERE status IN ('queued', 'retry');

CREATE TABLE ops.outbox_event (
    id uuid PRIMARY KEY,
    aggregate_type text NOT NULL,
    aggregate_id uuid NOT NULL,
    event_type text NOT NULL,
    payload jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    published_at timestamptz
);

CREATE OR REPLACE FUNCTION ops.reject_published_content_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF OLD.status = 'published' THEN
        RAISE EXCEPTION 'published % row % is immutable', TG_TABLE_NAME, OLD.id;
    END IF;
    RETURN CASE WHEN TG_OP = 'DELETE' THEN OLD ELSE NEW END;
END;
$$;

CREATE TRIGGER recipe_version_published_immutable
    BEFORE UPDATE OR DELETE ON recipe.recipe_version
    FOR EACH ROW EXECUTE FUNCTION ops.reject_published_content_mutation();

CREATE TRIGGER composition_profile_published_immutable
    BEFORE UPDATE OR DELETE ON composition.composition_profile
    FOR EACH ROW EXECUTE FUNCTION ops.reject_published_content_mutation();

CREATE OR REPLACE FUNCTION ops.reject_final_revision_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
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

CREATE TRIGGER analysis_revision_final_immutable
    BEFORE UPDATE OR DELETE ON analysis.analysis_revision
    FOR EACH ROW EXECUTE FUNCTION ops.reject_final_revision_mutation();

CREATE OR REPLACE FUNCTION ops.reject_final_revision_child_mutation()
RETURNS trigger
LANGUAGE plpgsql
AS $$
DECLARE
    target_revision_id uuid;
BEGIN
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

CREATE TRIGGER analysis_item_final_revision_immutable
    BEFORE INSERT OR UPDATE OR DELETE ON analysis.analysis_item
    FOR EACH ROW EXECUTE FUNCTION ops.reject_final_revision_child_mutation();
