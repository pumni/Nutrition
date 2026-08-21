# Privacy export ownership

## Starting state
Export/delete operations are authenticated and ownership-scoped.

## User task
Change privacy export or deletion behavior in a defined way.

## Expected behavioral outcome
Only the authenticated owner's data is exported/deleted, with approved audit and retention semantics preserved.

## Must not do
Do not broaden queries to all users, leak raw content, or weaken authorization.

## Verification
API privacy/ownership tests, privacy scan, and `cargo xtask postgres` when available.

## Human-decision expectation
none
