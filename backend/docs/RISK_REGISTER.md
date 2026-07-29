# Foundation risk register

| Risk | Probability | Impact | Current control | Required next action | Owner |
|---|---:|---:|---|---|---|
| Vietnamese dish evidence is unavailable or unusable | High | High | No production seed has been activated | Select sources and approve curation protocol | Unassigned data/domain owner |
| Portion uncertainty dominates estimates | High | High | Two test-only contextual observations propagate mass/nutrient bounds | Define 10 measures and run a real measurement study | Unassigned domain owner |
| Fixture data is mistaken for production evidence | Medium | High | Fixture adapter and source register mark it test-only | Enforce environment/catalog release activation | Engineering |
| Rust delivery velocity is too low | Medium | High | ADR-001 remains proposed | Measure next two vertical slices and onboarding | Tech lead |
| Raw meal text leaks through telemetry | Medium | High | HTTP traces exclude body; no raw-text logs | Add automated redaction/log capture tests | Security owner |
| Revision child rows are mutated after completion | Low | High | Database triggers, SQL verification, and repository integration tests | Expand adversarial concurrency coverage | Backend owner |
| Calculator behavior drifts silently | Low | High | Decimal arithmetic, engine version, golden tests | Add fixture manifest replay report in CI | Calculation owner |
| Curation becomes the delivery bottleneck | High | High | Publication is not implemented yet | Assign curator capacity before catalog expansion | Product owner |
| Runtime becomes coupled to fixture adapters | Medium | Medium | Catalog and persistence are PostgreSQL-backed; only parser is a fixture | Add hosted-parser adapter behind the existing port | Backend owner |
| Infrastructure expands before evidence | Medium | Medium | PostgreSQL-only foundation | Require ADR trigger for new infrastructure | Tech lead |
