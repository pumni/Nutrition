# Delivery roadmap

**Phiên bản:** 1.0.0  
**Nguyên tắc:** Vertical slices, evidence gates, product feedback trước platform expansion

## 1. Delivery model

Không triển khai theo lớp ngang kiểu “xây database xong, xây AI xong, cuối cùng mới có flow”. Mỗi phase phải tạo một đường chạy hoàn chỉnh:

```text
input → interpretation → evidence → calculation → output → correction → telemetry
```

Mọi phase có:

- scope cố định;
- owner;
- deliverables;
- measurable exit gate;
- explicit deferred list;
- retrospective cập nhật blueprint/ADR.

## 2. Team tối thiểu

| Trách nhiệm | Capacity khuyến nghị |
|---|---:|
| Tech lead/backend architect | 1 |
| Backend engineer | 1–2 |
| Data/curation engineer | 1 |
| Product/UX owner | 1 |
| Nutrition/domain reviewer | part-time |
| QA/AI evaluation | part-time hoặc shared |
| DevOps/security | part-time |

Một người có thể giữ nhiều vai trò, nhưng mỗi trách nhiệm phải có owner và backup.

## 3. Phase 0 — Discovery và evidence preparation

### Goal

Biến assumption thành danh sách có thể kiểm chứng trước khi chọn runtime và schema chi tiết.

### Work

- Interview/observe target users.
- Thu thập 50–100 câu meal text thật hoặc synthetic có review.
- Chọn 10 món đại diện cho edge cases.
- Source/license feasibility.
- Viết product scope, glossary, ADR proposed.
- Chốt product copy “estimate”.

### Deliverables

- Problem statement.
- Initial VietnameseMealBench.
- Source register draft.
- Risk register.
- Walking skeleton backlog.

### Exit gate

- Product owner duyệt scope/non-goals.
- Domain reviewer duyệt 10 món/fixtures.
- Không có source/license blocker không có mitigation.

## 4. Phase 0.5 — Walking skeleton

### Goal

Chứng minh kiến trúc end-to-end với phạm vi cực nhỏ.

### Scope

```text
20 basic foods
10 Vietnamese dishes
10 measures
50–100 texts
4 nutrients
1 source adapter
1 LLM adapter
1 clarification flow
1 correction flow
```

### Workstreams

#### Domain/data

- `food_entity`, names, source mapping.
- Minimal composition profile/value.
- Portion observation.
- Recipe/version/component một cấp.
- Analysis/revision/evidence snapshot.

#### Application

- Parse DTO/schema.
- Exact + trigram retrieval.
- Rule scoring.
- Resolve/clarify/insufficient state.
- Deterministic calculator.

#### Product

- Minimal input/result/correction interface.
- One-question clarification.
- Assumption disclosure.

#### Operations

- Local compose/dev environment.
- CI tests.
- Structured logs/traces.
- Backup/restore.

### Runtime decision spike

Implement cùng một vertical slice hoặc critical module prototype để đánh giá:

- Rust team velocity;
- build/CI ergonomics;
- SQLx migration/query workflow;
- LLM SDK/adapters;
- testability và observability.

Kết quả ghi vào ADR-001.

### Exit gate

- 20 end-to-end golden fixtures pass.
- Snapshot replay pass.
- Unknown không bị force-match.
- Correction append-only.
- Calculator offline pass.
- P95 prototype trong budget hoặc có optimization plan.
- Team chốt runtime.

## 5. Phase 1 — Internal alpha

### Goal

Tăng dữ liệu và biến skeleton thành sản phẩm dùng nội bộ hằng ngày.

### Scope

- 50 basic foods, 25 dishes, 15 measures.
- 150–250 texts.
- Curation review queue.
- Import/activation/rollback source release.
- Basic user meal history.

### Work

- Complete canonical mappings.
- Admin create/edit/merge/deprecate.
- Clarification state persistence.
- Correction analytics.
- Cost/latency/quality dashboards.
- Provider timeout/fallback.

### Exit gate

- Internal users hoàn thành flow không cần developer hỗ trợ.
- Curator xử lý mapping/alias qua UI.
- No blocker in data lifecycle.
- Error taxonomy ổn định đủ để mở rộng benchmark.

## 6. Phase 2 — Deterministic MVP beta

### Goal

Beta có coverage mục tiêu và governance tối thiểu.

### Scope

- 100–200 basic foods.
- 50–100 curated dishes.
- 20–30 measures.
- 300–500 initial labeled texts, liên tục tăng.
- Recipe versioning/nested depth bounded.
- Energy/macros; optional fiber/sodium.

### Workstreams

#### API/runtime

- Public versioned analyze API.
- Idempotency, auth/history.
- Clarification/correction endpoints.
- Rate limiting và abuse controls.

#### Data

- Multiple source adapters.
- Quality grade/selection policy.
- Dataset activation impact report.
- Provenance export.

#### Quality

- VietnameseMealBench release pipeline.
- External/provider baseline comparison.
- Human portion measurement protocol.
- Shadow mode analysis.

#### Product

- Refined result/assumption UI.
- Correction under interaction budget.
- Accessibility/localization review.

### Exit gate

- Product requirements beta acceptance đạt.
- Security/privacy review pass.
- Restore and rollback drill pass.
- Cost per finalized analysis within budget.
- Curation queue SLA sustainable.

## 7. Phase 3 — Vietnamese recipe intelligence

### Goal

Giảm lỗi trên composite dishes, modifiers và regional variants.

### Work

- Regional recipe variants.
- Add-on/removable components.
- Better edible portion/yield evidence.
- Nested recipes depth policy.
- Slang/brand/colloquial aliases.
- Recipe/profile impact preview.
- Cooking trials hoặc measured portions có protocol.

### Exit gate

- Correction rate giảm có ý nghĩa trên top dishes.
- Portion error giảm trên measured slice.
- Recipe calculations pass independent review.
- No cycle/version regression.

## 8. Phase 4 — Data-quality maturity

### Goal

Mở rộng nutrients và nâng data governance.

### Work

- Selected micronutrients.
- Retention/yield factors có evidence.
- Compiled/imputed profile policy.
- Data release calendar.
- Quality audit samples.
- Source freshness/anomaly detection.
- Calibrated resolution probabilities nếu đủ labels.

### Exit gate

- Completeness targets theo nutrient.
- Quality grades audited.
- Selection policy stable qua nhiều release.
- Calibration report chứng minh user-facing probability nếu có.

## 9. Phase 5 — Evidence-triggered expansion

Chỉ xem xét bằng ADR và benchmark:

- vector retrieval;
- LLM reranking;
- personalized portions;
- barcode/branded products;
- restaurant menus;
- image input;
- additional languages;
- service decomposition;
- specialized analytics/search storage.

Mỗi candidate phải trả lời:

1. Metric nào đang thất bại?
2. Baseline hiện tại là gì?
3. Giải pháp đơn giản hơn đã thử chưa?
4. Offline uplift bao nhiêu?
5. Online/product uplift bao nhiêu?
6. Privacy/cost/ops consequence là gì?

## 10. Epic map

### E-A Foundations

Repository, CI, local stack, IDs, units, errors, migrations, telemetry.

### E-B Catalog

Food identity, names, taxonomy, source records, mappings, search.

### E-C Composition

Nutrient vocabulary, profiles, values, portion observations, factors.

### E-D Recipe/calculation

Recipe/version/components, cycle checks, calculator, traces.

### E-E Language/resolution

Parser adapter, normalization, candidates, scoring, decision policy.

### E-F Analysis/product loop

Orchestration, clarification, correction, history, assumptions.

### E-G Data operations

Source adapters, release staging, activation, rollback, curation.

### E-H Quality/security/ops

Benchmark, dashboards, backup, privacy, incident runbooks.

## 11. Risk register

| Risk | Probability | Impact | Leading indicator | Mitigation |
|---|---|---|---|---|
| Thiếu dữ liệu món Việt | High | High | unresolved/correction tập trung | curated scope, domain partner |
| Portion uncertainty | High | High | portion MAE/correction | measurement study, clarify |
| Rust velocity thấp | Medium | High | missed skeleton milestones | spike, owner, runtime ADR |
| LLM drift | Medium | High | schema/behavior regression | pin/config registry, eval gate |
| License conflict | Medium | High | source review blocker | source registry, legal gate |
| Duplicate catalog | High | Medium | merge queue growth | canonical/search/review tooling |
| False precision | High | High | user trust issues | range/assumption UX |
| Curation bottleneck | High | High | queue age | scope, automation for triage only |
| External provider lock-in | Medium | Medium | provider-specific fields leak | anti-corruption layer |
| Overengineering | Medium | High | platform work before flow | phase gates/non-goals |

## 12. Build/buy/reuse

### Buy/managed

- PostgreSQL hosting.
- Object storage.
- Identity provider.
- LLM inference.
- Observability backend.
- Optional external nutrition API for baseline/shadow.

### Build

- Canonical food model.
- Vietnamese aliases/portion ontology.
- Recipe/version semantics.
- Composition selection.
- Deterministic calculator.
- Clarification/correction loop.
- Curation and benchmark.

### Reuse/reference

Theo `15_OPEN_SOURCE_AND_MARKET_REFERENCE_STRATEGY.md`; không copy code trước license/security review.

## 13. Release cadence

- Application: backward-compatible continuous delivery.
- Parser/model/prompt: behavior release có benchmark.
- Calculator: semantic version, fixtures và migration note.
- Catalog: versioned data release có impact report.
- Source import: theo upstream cadence; không auto-activate.
- Blueprint: cập nhật theo milestone hoặc accepted ADR.

## 14. Definition of launch-ready

- Product scope và safety copy approved.
- Benchmark/quality gates đạt.
- Clarification/correction flow usable.
- Catalog/source governance hoạt động.
- Security/privacy no blocker.
- Backup/restore/rollback pass.
- Monitoring, alerts, runbooks và owner on-call rõ.
- Cost và curation capacity sustainable.
