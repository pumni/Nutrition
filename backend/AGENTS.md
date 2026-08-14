# Coding Agent Entry Point

The harness compiles human Task Intent into a bounded execution spec. Find the machine-readable manifest at `.agent/manifest.json`, operating guidance at `docs/AGENT_ENGINEERING.md`, authority at `.agent/authority/`, and canonical gate IDs at `.agent/maps/verification-map.json`.

Human owners control product/domain semantics, architecture, public API, database and migration intent, dependencies, security/privacy, providers, behavior versions, infrastructure, publication, and release policy. The agent investigates, plans, implements, tests, debugs, and revises autonomously within the compiled scope envelope; it never silently chooses a protected change.

Start with `AGENTS.md` and the minimal modules routed by `.agent/context/router.json`. Expand context only when repository evidence requires it. Verify the actual diff with canonical gate IDs and protected-path approvals. The harness derives context and verification requirements from the observed outcome; task artifacts do not define commands.

If baseline, contract, scope, context, or verification preconditions conflict, stop only the affected work and produce a protected-decision report with classification, evidence, constraint, impact, and the smallest human decision required.
