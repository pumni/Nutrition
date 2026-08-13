# Decision Policy

Authority is ordered from highest to lowest:

1. explicit current instruction from the human owner or architect;
2. the architect-authored packet for the current task;
3. accepted project ADRs and foundation decisions;
4. product, domain, security, and privacy source specifications;
5. compiled invariants and task context;
6. existing implementation details;
7. executor preferences.

The executor does not use a lower layer to override a higher layer. The architect decides public contracts, dependency boundaries, persistence shape, security and privacy behavior, product/domain rules, provider behavior, infrastructure, and versioning.

If a packet leaves a required decision unresolved, the executor stops. It does not select a preferred alternative, widen scope, or make an implicit behavior/version/dependency/database choice.

Every task packet must make its decision points explicit. A completed packet has no unresolved decision points; the executor follows its stated paths, sequence, impacts, acceptance criteria, and verification.

Sources:

- `Nutrition_backend_agent_context_layer_plan/task_packets/P02_AUTHORITY_INVARIANTS.md`
- `docs/FOUNDATION_DECISIONS.md`
- `nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md`
