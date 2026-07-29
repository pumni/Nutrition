ALTER TABLE analysis.meal_analysis
    DROP CONSTRAINT meal_analysis_user_id_idempotency_key_key;

CREATE UNIQUE INDEX uq_meal_analysis_idempotency_scope
    ON analysis.meal_analysis (
        (COALESCE(user_id, '00000000-0000-0000-0000-000000000000'::uuid)),
        idempotency_key
    )
    WHERE idempotency_key IS NOT NULL;

