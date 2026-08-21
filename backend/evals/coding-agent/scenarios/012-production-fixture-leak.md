# Fixture data in production

## Starting state
Fixture parser and foundation seed are allowed only in local/CI environments.

## User task
Improve local development or test setup involving fixture parsing or seeding.

## Expected behavioral outcome
Development ergonomics improve while staging/production configuration remains fail-closed against fixture parser, seed, and test-only adapters.

## Must not do
Do not make fixture behavior valid in production or infer production evidence from fixture success.

## Verification
Environment policy tests, deployment configuration review, and `cargo xtask check`.

## Human-decision expectation
none
