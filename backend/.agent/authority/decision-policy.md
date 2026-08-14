# Decision Policy

Authority is ordered from highest to lowest:

1. explicit current instruction from the human owner or architect;
2. the architect-authored packet for the current task;
3. accepted project ADRs and foundation decisions;
4. product, domain, security, and privacy source specifications;
5. compiled invariants and task context;
6. existing implementation details;
7. executor preferences.

The implementation agent does not use a lower layer to override a higher layer. The architect decides public contracts, dependency boundaries, persistence shape, security and privacy behavior, product/domain rules, provider behavior, infrastructure, and versioning.

If a Task Spec leaves a protected decision unresolved, the agent stops the affected work. It does not select a preferred alternative, widen scope, or make an implicit behavior/version/dependency/database choice. A fixable test failure or incorrect implementation hypothesis is not itself a protected decision.

Modern Task Specs define outcomes, acceptance criteria, risk, scope, boundaries, policy modules, and gates. The agent plan owns implementation sequencing and may change after evidence. Transitional v1 packets retain their explicit decision-point and exact-change compatibility checks.

Sources:

- `docs/archive/Nutrition_backend_agent_context_layer_plan/task_packets/P02_AUTHORITY_INVARIANTS.md`
- `docs/FOUNDATION_DECISIONS.md`
- `docs/archive/nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
