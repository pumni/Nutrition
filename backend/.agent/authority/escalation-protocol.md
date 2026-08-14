# Escalation Protocol

Stop the affected work before writing when the approved task artifact is missing, the repository baseline is stale, a protected path or conflicting change is present, a required verification fails without an in-scope correction, or implementation requires an unspecified protected decision. Do not escalate merely because an initial implementation hypothesis or fixable test failed.

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

For a protected-decision blocker, the report must identify:

- the classification;
- the observed fact;
- the existing constraint;
- the exact file, path, symbol, test, or error evidence;
- why the current approved task cannot proceed;
- the implementation impact;
- the smallest architect decision required.

After escalation, do not continue the blocked change, create a workaround, or silently approve an option. Safe unrelated verification may continue only when it does not expand scope.

Sources:

- `docs/archive/Nutrition_backend_agent_context_layer_plan/task_packets/P02_AUTHORITY_INVARIANTS.md`
- `docs/FOUNDATION_DECISIONS.md`
- `docs/SECURITY_AND_OPERATIONS.md`
