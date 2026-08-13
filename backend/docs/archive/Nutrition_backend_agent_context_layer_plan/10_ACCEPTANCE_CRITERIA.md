# 10 — Acceptance Criteria

ACL v1 is accepted only when all criteria below are true.

## A. Scope integrity

- [ ] No file under `crates/**` changed.
- [ ] No file under `migrations/**` changed.
- [ ] `Cargo.toml` and `Cargo.lock` unchanged.
- [ ] No deploy/runtime config changed.
- [ ] No parser schema changed.
- [ ] No fixture/seed data changed.
- [ ] Application behavior release remains `foundation-0.6.0`.
- [ ] No new external dependency/tool installation required.

## B. Context architecture

- [ ] `AGENTS.md` exists and is <= 4096 bytes.
- [ ] `.agent/manifest.json` exists and identifies `agent-context-1.0.0`.
- [ ] Authority files exist.
- [ ] Five invariant files exist.
- [ ] Nine context packs exist.
- [ ] Four map/index artifacts exist.
- [ ] Nine profiles exist.
- [ ] Three contract schemas exist.
- [ ] Templates exist.
- [ ] Source lock exists.
- [ ] Evals exist.

## C. Authority enforcement

- [ ] Task packet is mandatory before executor writes.
- [ ] Context profile is mandatory.
- [ ] `decision_points` must be empty.
- [ ] Executor contract explicitly forbids architecture/product/security/database/version decisions.
- [ ] Block classifications are defined.
- [ ] Completion report requires impact declarations.

## D. Context quality

- [ ] Context is source-linked.
- [ ] Existing blueprint/docs remain canonical truth.
- [ ] No giant duplicated blueprint appears inside `.agent`.
- [ ] Context files obey byte budgets.
- [ ] Profiles load only relevant context.
- [ ] Vendor-specific prompt tricks are not canonical.

## E. Deterministic verification

- [ ] `verify-agent-context.ps1 -SelfTest` passes.
- [ ] `verify-agent-context.ps1` passes.
- [ ] All required negative evals fail for the expected reason.
- [ ] Stale source-lock case fails.
- [ ] Changed-file outside allowlist case fails.
- [ ] Non-empty decision point case fails.
- [ ] Missing/unknown profile cases fail.

## F. Repository integration

- [ ] Existing `scripts/verify.ps1` calls ACL verification first.
- [ ] Existing formatting check remains.
- [ ] Existing Clippy check remains.
- [ ] Existing workspace tests remain.
- [ ] Existing JSON validation remains.
- [ ] Existing sensitive-log scan remains.
- [ ] Existing Docker Compose validation remains.
- [ ] Full `scripts/verify.ps1` passes.

## G. Deletability

A reviewer can list exactly four removal actions that return the repo to pre-ACL behavior:
1. delete `AGENTS.md`;
2. delete `.agent/`;
3. delete `scripts/verify-agent-context.ps1`;
4. remove the ACL call and README section.

No runtime code cleanup is needed.

## H. User's control requirement

A reviewer can demonstrate:
- architect chooses profile;
- architect chooses allowed paths;
- architect decides all impacts;
- architect provides exact sequence;
- executor cannot proceed on missing decision;
- verifier catches scope widening.

This is the primary success criterion.
