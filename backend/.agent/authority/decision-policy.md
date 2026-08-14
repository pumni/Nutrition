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

Task Specs define outcomes, acceptance criteria, risk, scope, protected domains, policy modules, and verification gates. The agent plan owns implementation sequencing and may change after evidence. If a protected decision is unresolved or a Task Spec conflicts with a higher authority, stop the affected work and report the smallest decision needed.
