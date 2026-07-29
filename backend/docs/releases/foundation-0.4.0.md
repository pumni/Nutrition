# Foundation 0.4.0

Status: verified implementation increment  
Previous release: `foundation-0.3.0`

## Delivered

- One-turn portion clarification for a resolved single food with an unsupported unit.
- Clarification options sourced from active-release portion observations.
- Clarification answers create revision 2 without invoking the parser.
- Portion correction by item index creates an append-only completed revision.
- `FOR UPDATE` current-revision locking and explicit stale clarification/revision conflicts.
- Database-enforced analysis status transitions.
- Append-only clarification answers and analysis corrections.
- Hash-verified revision-history endpoint.
- Transactional create/correction idempotency records with immutable response replay.
- Same-key/different-body `409 idempotency_conflict`.
- Independent clarification and correction behavior versions pinned per revision.

## API

- `POST /v1/nutrition/analyses/{id}/clarifications`
- `POST /v1/nutrition/analyses/{id}/corrections`
- `GET /v1/nutrition/analyses/{id}/revisions/{revision_number}`

Create and correction accept `Idempotency-Key`.

## Current workflow boundary

The clarification policy handles one unsupported portion dimension for one otherwise resolved
food. Correction changes quantity/unit for existing items; food replacement, add/remove item,
modifier, consumed fraction, and revert remain future vertical slices. The active parser and
catalog evidence are still development fixtures.

## Verification

- Formatting, Clippy with warnings denied, unit tests, and JSON validation.
- Nine forward-only SQLx migrations.
- PostgreSQL completed, clarification, answer, correction, stale replay, and history integration.
- HTTP create/read, idempotent replay/conflict, clarification, correction, and revision history.
- Existing catalog and analysis immutability suite.

## Next increment

`foundation-0.5.0` adds reliable worker/outbox processing, authentication/privacy boundaries, and
operational contracts.
