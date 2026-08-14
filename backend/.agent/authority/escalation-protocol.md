# Escalation Protocol

Stop before writing when the task packet is missing, the repository baseline or expected implementation does not match, a forbidden path or conflicting change is present, a required verification fails, or a packet requirement forces an unspecified decision.

Use these exact classifications:

- `BLOCKED_TASK_PACKET_REQUIRED`
- `BLOCKED_BASELINE_DRIFT`
- `BLOCKED_DECISION: CONTEXT_PROFILE_REQUIRED`
- `BLOCKED_DECISION: UNSPECIFIED_PUBLIC_CONTRACT`
- `BLOCKED_DECISION: UNSPECIFIED_DEPENDENCY_CHANGE`
- `BLOCKED_DECISION: UNSPECIFIED_DATABASE_CHANGE`
- `BLOCKED_DECISION: UNSPECIFIED_BEHAVIOR_VERSION`
- `BLOCKED_IMPLEMENTATION_MISMATCH`
- `BLOCKED_VERIFICATION_FAILURE`
- `BLOCKED_SCOPE_CONFLICT`

The report must identify:

- the observed fact;
- the packet requirement that conflicts or is missing;
- the exact file, path, or symbol;
- the smallest architect decision or repository correction required.

After escalation, do not continue implementation, do not create a workaround, and do not propose an architecture unless the architect separately requests options.

Sources:

- `docs/archive/Nutrition_backend_agent_context_layer_plan/task_packets/P02_AUTHORITY_INVARIANTS.md`
- `docs/FOUNDATION_DECISIONS.md`
- `docs/SECURITY_AND_OPERATIONS.md`
