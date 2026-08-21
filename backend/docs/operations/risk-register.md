# Foundation risk register

| Risk | Probability | Impact | Current control | Required next action | Owner |
|---|---:|---:|---|---|---|
| Vietnamese dish evidence is unavailable or unusable | High | High | No production seed has been activated | Select sources and approve curation protocol | Unassigned data/domain owner |
| Portion uncertainty dominates estimates | High | High | Two test-only contextual observations propagate mass/nutrient bounds | Define 10 measures and run a real measurement study | Unassigned domain owner |
| Fixture data is mistaken for production evidence | Medium | High | Fixture adapter and source register mark it test-only | Enforce environment/catalog release activation | Engineering |
| Rust delivery velocity is too low | Medium | High | ADR-001 remains proposed | Measure next two vertical slices and onboarding | Tech lead |
| Raw meal text leaks through telemetry or provider diagnostics | Medium | High | HTTP traces exclude body; static scan passes; hosted telemetry schema has no content fields; mock request privacy test | Add runtime log-capture test and approve provider retention settings | Security owner |
| Hosted provider retains or trains on meal text | Medium | High | Hosted mode is explicit and has a provider-neutral minimum-data envelope | Legal/security approval of provider terms, residency, retention, and training opt-out | Security owner |
| Prompt injection creates fabricated consumed foods | Medium | High | Meal text is labeled untrusted; strict schema plus source-span, duplicate, and negation checks; no provider tools | Run adversarial Vietnamese benchmark before enabling hosted mode | ML/security owner |
| Provider outage exhausts API capacity | Medium | High | 3 s default timeout, one retry maximum, bounded response stream, provider/model circuit breaker | Tune with staging latency/error telemetry and alerting | Backend owner |
| Development bearer auth is enabled outside local/CI | Medium | High | Non-development auth mode fails startup; OIDC adapter enforces HTTPS issuer/audience, RS256-only validation, JWKS discovery/cache/refresh, and fail-closed claims checks | Select and approve the production identity provider; verify provider configuration in staging; complete deployment/security review | Security owner |
| Worker side effect is repeated after crash | Medium | High | Bounded leases/retries and idempotent foundation handlers | Add external sink idempotency key and crash-injection tests | Backend owner |
| Revision child rows are mutated after completion | Low | High | Database triggers, SQL verification, and repository integration tests | Expand adversarial concurrency coverage | Backend owner |
| Concurrent clarification/correction overwrites history | Low | High | Row lock, expected revision check, append-only revisions, stale-request tests | Add load/concurrency stress test | Backend owner |
| Calculator behavior drifts silently | Low | High | Decimal arithmetic, engine version, golden tests | Add fixture manifest replay report in CI | Calculation owner |
| Curation becomes the delivery bottleneck | High | High | Publication is not implemented yet | Assign curator capacity before catalog expansion | Product owner |
| Runtime silently changes from hosted to fixture behavior | Low | High | Required typed parser mode; no fallback; persisted provider/model and prompt/schema versions | Add deployment policy preventing fixture mode outside local/CI | Platform owner |
| Infrastructure expands before evidence | Medium | Medium | PostgreSQL-only foundation | Require ADR trigger for new infrastructure | Tech lead |
