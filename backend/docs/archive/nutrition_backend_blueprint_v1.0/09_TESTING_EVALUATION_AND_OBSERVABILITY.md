# Testing, evaluation và observability

**Phiên bản:** 1.0.0  
**Trạng thái:** Release-quality specification  
**Primary benchmark:** VietnameseMealBench


## 1. Quality strategy

Hệ thống có hai loại correctness:

1. **Software correctness:** code, transaction, unit conversion, security.
2. **Semantic/model correctness:** parse đúng, resolve đúng, portion hợp lý, source phù hợp.

Cả hai phải có test suite độc lập. Uptime tốt không bù được estimate sai.

## 2. Test pyramid

### Unit tests

- Domain value objects.
- Unit conversion.
- Calculation engine.
- Recipe graph/cycle.
- Scoring policy.
- Quality selection.
- Rounding/missing semantics.

### Property-based tests

- Scale mass lên `k` thì direct nutrient scale `k`.
- Total bằng sum items trong tolerance.
- Bounds không đảo.
- Không kết quả âm với nonnegative input.
- Conversion round-trip trong tolerance.
- Recipe ordering không thay total.

### Integration tests

- SQL repositories với real PostgreSQL.
- Migrations.
- Partial unique indexes.
- Concurrent job claims.
- Idempotency.
- Publish workflow.
- Analysis persistence/reload/recalculate.

### Contract tests

- LLM adapter mock/recorded schemas.
- Object storage adapter.
- External dataset parser với frozen sample.
- HTTP OpenAPI request/response.

### End-to-end tests

- Simple direct food.
- Composite dish.
- Clarification.
- Correction/revision.
- LLM timeout.
- Unknown food.
- Dataset publish and catalog switch.

## 3. Golden evaluation corpus

Mỗi case chứa:

```json
{
  "id": "vi-001",
  "input": "ăn hai quả trứng luộc và một bát cơm",
  "annotations": {
    "mentions": [],
    "resolved_foods": [],
    "quantities": [],
    "expected_decision": "resolved",
    "acceptable_candidates": [],
    "calculation_fixture": null
  },
  "tags": ["vi", "count-unit", "multi-item"]
}
```

Corpus versioned; annotation changes có reviewer và rationale.

## 4. Dataset splits

- Train/development: dùng tune rules/prompts.
- Validation: chọn threshold/policy.
- Test: khóa, không dùng tune.
- Challenge set: adversarial, slang, negation, rare regional terms.
- Production shadow set: sampled/redacted theo privacy policy.

Không report metric trên cùng set dùng tối ưu prompt.

## 5. Annotation protocol

- Hai annotators cho ambiguous cases.
- Adjudication bởi domain reviewer.
- Cho phép nhiều acceptable food candidates nếu input thực sự không đủ.
- Gán nhãn `needs_clarification` thay vì ép một ground truth.
- Ghi annotation confidence và note.

## 6. Parser metrics

- Mention precision/recall/F1.
- Exact span F1 và relaxed overlap F1.
- Quantity accuracy.
- Unit accuracy.
- Negation accuracy.
- Modifier extraction F1.
- Schema-valid rate.
- Hallucinated-item rate.

Hallucinated-item rate là guardrail quan trọng, không chỉ F1 tổng.

## 7. Resolution metrics

- Top-1 accuracy.
- Top-3 recall.
- Mean reciprocal rank.
- Unknown rejection precision/recall.
- Clarification precision: tỷ lệ hỏi lại khi thực sự cần.
- Over-resolution rate: resolve khi phải unknown/clarify.
- Under-resolution rate: hỏi lại khi exact match đủ.

Slice theo:

- region.
- diacritic.
- brand.
- preparation.
- food type.
- alias frequency.

## 8. Portion metrics

Ground truth khó hơn; dùng multiple evaluation modes:

### Controlled fixtures

Known grams/servings từ curated evidence.

### Human measurement study

Nếu có cooking/portion trial:

- MAE gram.
- Median absolute percentage error.
- Bound coverage: observed gram có nằm trong returned bounds không.
- Bound width.

Không tối ưu coverage bằng range quá rộng; theo dõi cả coverage và width.

## 9. Nutrition estimate metrics

Trên recipes/foods có reference:

- Absolute error.
- MAPE, nhưng tránh khi denominator gần zero.
- Symmetric MAPE hoặc weighted absolute percentage error.
- Macro-specific error.
- Energy error.
- Completeness-aware metric.

Tách lỗi:

```text
total error
├── parser/resolution
├── portion
├── composition selection
└── calculation
```

Calculation engine trên exact inputs phải gần như zero error so với fixture; product estimate error chủ yếu từ upstream uncertainty.

## 10. Calibration

Nếu sau này trả numeric probability:

- Reliability diagram.
- Expected calibration error.
- Brier score.
- Calibration theo slices.

Trước đó chỉ dùng labels dựa trên evidence policy: high/medium/low/insufficient.

## 11. Regression gates

Một prompt/model/policy release bị block nếu:

- Hallucinated-item rate tăng vượt guardrail.
- Unknown over-resolution tăng.
- Top-1 giảm vượt tolerance.
- Schema-valid giảm.
- Latency/cost vượt budget.
- Critical challenge case fail.

Chấp nhận trade-off chỉ với signed release note/ADR.

## 12. Load/performance tests

Workloads:

- Candidate search trên realistic catalog size.
- 10-item request.
- Concurrent LLM calls.
- Analysis persist transaction.
- Import release.
- Job claim contention.
- Curation search.

Đo:

- P50/P95/P99.
- DB pool wait.
- Query time/rows.
- Memory/CPU.
- LLM saturation.
- Error/timeouts.

## 13. Database tests

- Query plans snapshot cho critical queries.
- Trigram index used.
- No sequential scan regression trên large tables khi không mong muốn.
- Recursive recipe depth/cycle.
- Concurrent publish conflict.
- Unique active preferred name.
- Immutable published version.
- Restore/reproducibility.

## 14. Security tests

- Prompt injection corpus.
- SQL injection/parameterization.
- Broken object-level authorization.
- Rate-limit bypass.
- Oversized payload.
- Secret/log scan.
- User deletion.
- Audit tampering access.

## 15. Observability model

### Traces

Top-level span:

```text
POST /v1/nutrition/analyses
├── input.validate
├── parser.call
├── parser.semantic_validate
├── resolver.candidate_query
├── resolver.score
├── portion.resolve
├── composition.select
├── calculator.run
└── analysis.persist
```

Attributes không chứa raw meal text. Có:

- request ID.
- anonymous/user class.
- locale.
- item count.
- parser model/version.
- catalog release.
- result status.
- quality label.

### Metrics

Operational:

- request count/error/latency.
- provider latency/errors/tokens.
- DB pool/query latency.
- job age/failure.

Quality:

- unresolved rate.
- clarification rate.
- correction rate.
- low-quality evidence usage.
- missing nutrient rate.
- fallback parser usage.

Cost:

- token cost per analysis.
- storage growth.
- import compute.

### Logs

Structured JSON; levels; request/trace IDs. Không log raw prompts, user text, full provider response hoặc nutrient evidence payload mặc định.

## 16. Dashboards

### API health

Traffic, latency, errors, saturation.

### AI/parser

Provider errors, schema failures, token cost, fallback, semantic rejection.

### Data quality

Unknown foods, corrections, approximate mappings, source freshness, low-grade selections.

### Jobs/import

Queue age, retries, release progress, mapping review backlog.

## 17. Alerts

Page-worthy:

- Sustained high 5xx.
- DB unavailable/pool exhaustion.
- Analysis persistence failures.
- Security/auth anomaly.

Ticket/business-hour:

- Correction spike.
- Unknown-food spike.
- Source freshness lag.
- Cost drift.
- Evaluation regression.

Không page vì single LLM timeout.

## 18. Experimentation

A/B test only sau offline gate. Assignment ổn định theo user/session. Lưu variant trên analysis revision.

Guardrails:

- Hallucination.
- Over-resolution.
- Latency.
- Cost.
- User correction.

Không chỉ tối ưu “completion rate” nếu model tự tin sai.

## 19. Definition of Done cho feature

- Domain/API spec cập nhật.
- Unit/integration tests.
- Evaluation cases.
- Telemetry.
- Migration/rollback.
- Security/privacy review khi chạm data.
- Runbook.
- Release note/version impact.

## 20. v1.0 VietnameseMealBench integration

`16_VIETNAMESE_MEAL_BENCH_SPEC.md` is the normative benchmark definition. This document owns execution infrastructure, dashboards and release enforcement.

Pipeline:

```text
build behavior bundle
→ run unit/integration
→ run public regression set
→ run sealed release set
→ compare previous/baselines
→ generate signed report
→ release decision
```

## 21. External baseline comparison

Where terms permit, compare internal pipeline to:

- rule-only;
- direct LLM research baseline;
- ingredient parser baseline;
- external nutrition provider;
- previous release.

Do not optimize to one external provider's opaque output as ground truth.

## 22. Product-loop observability

Add metrics:

```text
clarification_trigger_rate
clarification_answer_rate
clarification_abandonment_rate
mean_clarification_turns
confirmation_without_correction_rate
correction_rate_by_food_slice
time_to_finalized_analysis
curation_queue_age
```

Raw meal text must not be metric label/log field.

## 23. Fitness-function CI checks

- No zero-byte markdown/spec files.
- Markdown fence balance.
- Internal link check.
- Domain dependency check.
- Published immutability test.
- Snapshot replay test.
- Import idempotency test.
- Migration compatibility test.

## 24. SLO error budgets

Define availability/latency windows and burn-rate alerts after beta traffic is representative. Quality error budget is separate from availability: a fast incorrect estimate is not a successful request for product quality reporting.
