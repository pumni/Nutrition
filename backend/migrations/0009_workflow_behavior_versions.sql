ALTER TABLE analysis.analysis_revision
    ADD COLUMN clarification_policy_version text NOT NULL
        DEFAULT 'clarification-none-0.1.0',
    ADD COLUMN correction_policy_version text NOT NULL
        DEFAULT 'correction-none-0.1.0';
