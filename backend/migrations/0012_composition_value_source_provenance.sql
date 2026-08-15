ALTER TABLE composition.composition_value
    ADD COLUMN source_nutrient_id bigint,
    ADD COLUMN source_method text,
    ADD COLUMN source_metadata jsonb NOT NULL DEFAULT '{}'::jsonb;

ALTER TABLE composition.composition_value
    ADD CONSTRAINT composition_value_source_nutrient_id_positive
    CHECK (source_nutrient_id IS NULL OR source_nutrient_id > 0);
