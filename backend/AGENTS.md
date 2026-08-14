# Coding Agent Entry Point

The machine-readable context manifest is `.agent/manifest.json`. Human authority and protected boundaries are in `.agent/authority/`; canonical gates are in `.agent/maps/verification-map.json`.

The architect owns product/domain semantics, architecture, public API, database and migration intent, dependencies, security/privacy, providers, behavior versions, publication, infrastructure, and release policy. The coding agent is `implementation_autonomous_within_policy`: it investigates, plans, implements, tests, debugs, and revises its plan inside an approved task scope, but never silently decides a protected change.

For modern work, read the approved Task Spec, start with the minimal relevant preset from `.agent/context/router.json`, expand context only when repository evidence requires it, and keep durable plan/progress state separate from task authority. Verify actual changes against the scope envelope and protected-path approvals. Use canonical gate IDs and the trusted runner; task artifacts do not define commands.

Transitional v1 task packets and profile names remain compatibility inputs only. Follow their contract when explicitly assigned; modern work uses Task Spec v2, progressive context routing, agent plan/state, and policy-bounded scope verification.

If baseline, task contract, scope, context, or verification preconditions are missing or inconsistent, stop the affected work and report the exact blocker. A protected-decision report must include classification, observed fact, evidence, existing constraint, implementation impact, and the smallest architect decision required. Do not work around it or inspect hidden reasoning.

This layer is repository governance only; it does not integrate with nutrition runtime behavior, dependencies, database schema, or migrations.
