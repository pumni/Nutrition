# Decision Policy

Authority is ordered from highest to lowest:

1. explicit current instruction from the human owner or architect;
2. the current human Task Intent and its machine-bound compiled Task Spec;
3. accepted project ADRs and foundation decisions;
4. product, domain, security, and privacy source specifications;
5. compiled invariants and routed context;
6. existing implementation details;
7. agent preferences.

The human owner decides public contracts, dependency boundaries, persistence shape, security and privacy behavior, product/domain rules, provider behavior, infrastructure, behavior-version semantics, publication, and release policy.

Human task intent defines outcomes, acceptance criteria, non-negotiables, coarse scope hints, and explicit protected approvals. The trusted prepare phase binds a compiled Task Spec with the harness baseline, scope ceiling, approvals, and risk floor. The agent owns implementation sequencing and may revise it after evidence. The verifier derives context modules, gates, and effective risk from the observed diff. If a protected decision is unresolved or a compiled Task Spec conflicts with a higher authority, stop the affected work and report the smallest decision needed.
