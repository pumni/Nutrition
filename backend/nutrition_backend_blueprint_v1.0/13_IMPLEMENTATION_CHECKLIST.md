# Implementation checklist

**Phiên bản:** 1.0.0  
**Cách dùng:** Checklist này là release gate, không phải danh sách tham khảo. Mỗi mục phải có owner, evidence link và trạng thái.

## A. Product/domain readiness

- [ ] `02_PRODUCT_SCOPE_AND_REQUIREMENTS.md` được product owner duyệt.
- [ ] Primary persona và use cases được xác nhận.
- [ ] Non-goals được đưa vào backlog policy.
- [ ] Domain glossary thống nhất trong code/API/data.
- [ ] Product copy dùng “ước tính”, không dùng “chính xác”.
- [ ] Domain/safety reviewer duyệt result wording.
- [ ] Open assumptions có owner và validation plan.

## B. ADR và architecture governance

- [ ] ADR-001 runtime được chốt sau spike.
- [ ] ADR-002…ADR-020 được team review.
- [ ] Dependency rules được enforce trong CI.
- [ ] Không có framework/database SDK trong domain crate.
- [ ] Architecture fitness functions có test hoặc runbook.
- [ ] Decision log link từ relevant pull requests.

## C. Repository và engineering baseline

- [ ] Monorepo/workspace layout.
- [ ] Formatting, lint, test, dependency audit trong CI.
- [ ] Reproducible local environment.
- [ ] Secrets không nằm trong repository/image.
- [ ] Config schema validated at startup.
- [ ] Error taxonomy và correlation ID.
- [ ] Clock/ID/provider abstraction cho tests.

## D. Database

- [ ] PostgreSQL supported version pinned.
- [ ] Logical schemas created.
- [ ] Migration convention documented.
- [ ] Expand/migrate/contract plan cho destructive changes.
- [ ] Constraints cho status/version/value semantics.
- [ ] Unique indexes cho aliases/source IDs/version numbers.
- [ ] Search indexes và query plans reviewed.
- [ ] Recipe cycle/depth validation.
- [ ] Published rows immutable by permission/trigger/application guard.
- [ ] Backup/restore test.
- [ ] Least-privilege roles.

## E. Seed/source data

- [ ] Source/license register.
- [ ] Source artifact checksum.
- [ ] 20 foods/10 dishes/10 measures cho skeleton.
- [ ] Nutrient vocabulary IDs/units reviewed.
- [ ] Missing/zero/trace fixtures.
- [ ] Source mappings reviewed.
- [ ] No automatic activation of upstream release.
- [ ] Raw records preserved.

## F. Calculation engine

- [ ] Unit newtypes/canonical units.
- [ ] Direct-food calculation.
- [ ] Edible fraction.
- [ ] Portion-to-grams conversion evidence.
- [ ] Recipe yield.
- [ ] Nested recipe depth/cycle behavior.
- [ ] Missing nutrient propagation.
- [ ] Declared vs calculated energy policy.
- [ ] Range/bounded uncertainty policy.
- [ ] Rounding only at output.
- [ ] Calculation trace.
- [ ] Engine semantic version.
- [ ] Pure tests without network/DB/LLM.

## G. Parser/resolver

- [ ] Input normalization keeps raw text.
- [ ] Structured schema versioned.
- [ ] Prompt injection controls.
- [ ] Provider timeout/retry/fallback.
- [ ] Semantic validation.
- [ ] Exact alias lookup.
- [ ] Trigram/full-text candidates.
- [ ] Rule-based features explainable.
- [ ] Unknown/ambiguity decision.
- [ ] Portion resolution policy.
- [ ] Negation/partial-consumption tests.
- [ ] No nutrient fields accepted from parser.

## H. Clarification/correction

- [ ] Persisted analysis state machine.
- [ ] One dimension per clarification turn.
- [ ] Options include other/unknown.
- [ ] Interaction budget enforced.
- [ ] Resume idempotently.
- [ ] Correction creates revision.
- [ ] Recalculation uses corrected evidence.
- [ ] Revision history readable.
- [ ] Analytics events exclude raw text.

## I. API/runtime

- [ ] Versioned endpoints.
- [ ] Idempotency keys.
- [ ] Auth/authorization where needed.
- [ ] Request/body limits.
- [ ] Timeouts and backpressure.
- [ ] No DB transaction held across LLM/network call.
- [ ] Graceful degradation.
- [ ] API errors do not leak internals.
- [ ] Provider anti-corruption adapter.
- [ ] OpenAPI/contract tests.

## J. Curation/admin

- [ ] Search canonical/source records.
- [ ] Create/edit draft.
- [ ] Mapping review queue.
- [ ] Alias management.
- [ ] Merge with redirect/impact preview.
- [ ] Recipe/profile/portion diff.
- [ ] Publish/deprecate.
- [ ] Audit reason/actor/time.
- [ ] Role separation.
- [ ] Queue age dashboard.

## K. Source adapters/jobs

- [ ] Adapter contract implemented.
- [ ] Release discovery/acquisition.
- [ ] Checksum/signature where available.
- [ ] Schema drift detection.
- [ ] Idempotent parse/load.
- [ ] Validation/data-quality report.
- [ ] Activation pointer.
- [ ] Rollback tested.
- [ ] Job retries/dead-letter behavior.
- [ ] Outbox transaction test.

## L. Evaluation

- [ ] VietnameseMealBench manifest/version.
- [ ] Train/dev/test separation.
- [ ] Annotation guideline and adjudication.
- [ ] Parser, resolution, portion, nutrition metrics.
- [ ] Unknown/negation/region slices.
- [ ] External/provider baseline.
- [ ] Release regression thresholds.
- [ ] Golden calculation fixtures.
- [ ] Shadow mode report if provider used.
- [ ] No test leakage into prompt tuning.

## M. Observability/operations

- [ ] Traces across all stages.
- [ ] Metrics for latency/cost/quality/corrections.
- [ ] Redacted structured logs.
- [ ] API, AI, data, job dashboards.
- [ ] Alert routing and owners.
- [ ] Provider outage runbook.
- [ ] Database restore runbook.
- [ ] Source rollback runbook.
- [ ] Capacity/cost alerts.

## N. Security/privacy/safety

- [ ] Threat model reviewed.
- [ ] Dependency/container scanning.
- [ ] TLS/encryption configuration.
- [ ] Data minimization.
- [ ] Raw text logging disabled.
- [ ] Provider data-use/retention reviewed.
- [ ] Export/delete flow tested.
- [ ] Rate limits/abuse controls.
- [ ] Dataset supply-chain validation.
- [ ] Estimate/disclaimer copy approved.
- [ ] No diagnostic/prescriptive output.

## O. Walking skeleton exit gate

- [ ] 20 end-to-end fixtures pass.
- [ ] Replay produces equivalent result.
- [ ] Unknown food not force-matched.
- [ ] One clarification flow completes.
- [ ] One correction flow creates revision.
- [ ] Backup/restore pass.
- [ ] Runtime ADR accepted or fallback selected.

## P. Beta launch gate

- [ ] Product acceptance criteria met.
- [ ] VietnameseMealBench release gate met.
- [ ] P95 latency/availability observed over agreed period.
- [ ] Cost per finalized analysis within budget.
- [ ] No critical/high security blocker.
- [ ] Curator SLA/capacity accepted.
- [ ] Rollback drills pass.
- [ ] Release/change records complete.
