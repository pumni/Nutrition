# P02 — Authority and Invariants

## Objective

Create the stable authority and invariant layer.

## Allowed paths

- `.agent/authority/**`
- `.agent/invariants/**`

## Exact files

Authority:
- `executor-contract.md`
- `decision-policy.md`
- `escalation-protocol.md`

Invariants:
- `product-domain.md`
- `architecture.md`
- `data-replay.md`
- `llm-boundary.md`
- `security-privacy.md`

## Source requirements

Use only:
- this implementation pack;
- `docs/FOUNDATION_DECISIONS.md`;
- `docs/HOSTED_PARSER.md`;
- `docs/RISK_REGISTER.md`;
- `docs/SECURITY_AND_OPERATIONS.md`;
- blueprint `00_README.md`;
- blueprint ADR document.

Every invariant file lists its source paths.

## Forbidden decisions

Do not add a new invariant not supported by listed sources/plan.

## Acceptance

- exactly 8 files created;
- all within budgets;
- block codes exactly match plan;
- no runtime files changed.
