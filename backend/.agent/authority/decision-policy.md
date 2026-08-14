# Decision Policy

Authority is ordered from highest to lowest:

1. explicit current instruction from the human owner or architect;
2. the approved Task Spec for the current work;
3. accepted project ADRs and foundation decisions;
4. product, domain, security, and privacy source specifications;
5. compiled invariants and routed context;
6. existing implementation details;
7. agent preferences.

The human owner decides public contracts, dependency boundaries, persistence shape, security and privacy behavior, product/domain rules, provider behavior, infrastructure, behavior-version semantics, publication, and release policy.

Human task intent defines outcomes, acceptance criteria, non-negotiables, scope hints, and explicit protected approvals. The trusted harness compiles the Task Spec, baseline, routed modules, atomic gates, scope authorization, and risk floor. Agent execution state may raise risk with repository evidence but cannot lower that floor. The agent plan owns implementation sequencing and may change after evidence. If a protected decision is unresolved or a compiled Task Spec conflicts with a higher authority, stop the affected work and report the smallest decision needed.
