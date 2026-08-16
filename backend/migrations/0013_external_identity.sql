CREATE SCHEMA auth;

CREATE TABLE auth.external_identity (
    issuer text NOT NULL CHECK (btrim(issuer) <> ''),
    subject text NOT NULL CHECK (btrim(subject) <> ''),
    user_id uuid NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (issuer, subject),
    UNIQUE (user_id)
);
