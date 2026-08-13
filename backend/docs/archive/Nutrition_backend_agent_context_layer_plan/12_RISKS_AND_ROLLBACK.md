# 12 — Risks and Rollback

## Risk R1 — Context duplication drifts

**Mitigation**
- source-linked summaries;
- source register;
- hash lock for governance sources;
- do not copy full blueprint text.

## Risk R2 — AGENTS.md becomes a giant system prompt

**Mitigation**
- 4096-byte hard budget;
- profiles;
- module context packs;
- task packets.

## Risk R3 — Executor becomes blocked too often

This is an intentional early bias.

A block is preferable to invented architecture. After real usage, the architect may reduce friction only using observed block data and explicit ACL version changes.

Do not relax authority rules preemptively.

## Risk R4 — False sense of security

ACL does not replace:
- code review;
- tests;
- CI;
- security review;
- database review;
- product/domain review.

It narrows executor discretion and improves context/verification.

## Risk R5 — Vendor coupling

**Mitigation**
- canonical Markdown/JSON/PowerShell;
- no vendor-specific model features in source of truth;
- vendor wrappers optional and thin.

## Risk R6 — Verifier is bypassed

**Mitigation**
- integrate in `scripts/verify.ps1`;
- task packet requires exact commands;
- final report records evidence;
- CI can later run the same script.

## Risk R7 — Source-lock causes noisy failures

Lock only governance sources that compile into ACL summaries, not all source code.

## Risk R8 — Allowlist patterns accidentally too broad

Initial packets must prefer exact file paths over broad globs.

Examples:
- prefer `scripts/verify.ps1`
- avoid `scripts/**` unless required.

## Rollback

ACL v1 has no runtime/database migration, so rollback is simple.

1. Remove `AGENTS.md`.
2. Remove `.agent/`.
3. Remove `scripts/verify-agent-context.ps1`.
4. Remove ACL invocation from `scripts/verify.ps1`.
5. Remove `README.md` ACL section.
6. Run original repository verification.

No data migration, rollback release, or application behavior version change is needed.
