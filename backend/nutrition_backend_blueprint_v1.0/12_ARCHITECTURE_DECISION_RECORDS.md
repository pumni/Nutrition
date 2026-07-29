# Architecture Decision Records

**Phiên bản:** 1.0.0  
**Quy ước trạng thái:** `proposed`, `accepted`, `superseded`, `rejected`, `deferred`  
**Quy tắc:** Mỗi ADR phải có context, decision, consequence, verification và reconsider trigger

## ADR-001 — Backend runtime language

**Status:** Proposed, phải chốt sau walking skeleton.

### Context

Rust mang lại type safety, predictable runtime, unit newtypes và deployment gọn; nhưng delivery risk phụ thuộc kinh nghiệm team. Domain correctness quan trọng hơn benchmark throughput.

### Decision

Dùng Rust + Tokio + Axum + SQLx cho walking skeleton. Chuyển `accepted` nếu team đạt milestone và code review/operations an toàn. Kotlin hoặc TypeScript là fallback hợp lệ; Python chỉ làm service ML riêng khi cần.

### Consequences

- Cần Rust owner.
- Compile time/SQLx workflow phải được đo.
- Domain/database blueprint không được phụ thuộc Rust-specific semantics.

### Verification

Walking skeleton lead time, defect rate, onboarding, test ergonomics, deployment drill.

### Reconsider trigger

Hai milestone liên tiếp chậm chủ yếu do language/tooling hoặc không có owner đủ năng lực.

## ADR-002 — Modular monolith

**Status:** Accepted.

### Decision

Một codebase, API/worker process types, một PostgreSQL database với logical schemas. Module theo bounded context.

### Consequences

- Transaction đơn giản.
- Domain refactor nhanh.
- Cần dependency rules để tránh big ball of mud.

### Reconsider trigger

Independent scaling/ownership/release/fault isolation được chứng minh.

## ADR-003 — PostgreSQL as primary source of truth

**Status:** Accepted.

### Decision

PostgreSQL 18 supported release làm primary store. Sử dụng relational model, JSONB cho raw metadata có giới hạn, full-text/`pg_trgm` cho search MVP, recursive CTE cho recipe dependency.

### Consequences

- Một backup/restore model.
- Không dùng graph/vector/search store sớm.
- Cần index/query review và data lifecycle.

### Reconsider trigger

Measured workload không đạt SLO sau schema/query/index optimization.

## ADR-004 — Unified `food_entity`

**Status:** Accepted.

### Decision

Nguyên liệu, món, processed food và branded product dùng chung identity table với `entity_kind`; ingredient là role trong recipe component.

### Consequences

- Reuse/composition tự nhiên.
- Tránh mapping giữa ingredient/food silos.
- Validation phải theo entity kind và facets.

## ADR-005 — Food concept separated from source records

**Status:** Accepted.

### Decision

Upstream record không phải canonical food. Mapping có method, score, review status và release provenance.

### Consequences

- Hỗ trợ nhiều source cùng một concept.
- Cần curator workflow và merge/deprecate.

## ADR-006 — Composition profile as evidence

**Status:** Accepted.

### Decision

Nutrient values nằm trong versioned composition profiles; profile type gồm measured, manufacturer label, compiled, recipe-calculated, imputed.

### Consequences

- Không có `food_nutrients` phẳng.
- Selection policy là domain service có version.
- Missing/zero/trace semantics được bảo toàn.

## ADR-007 — Recipe and recipe version separation

**Status:** Accepted.

### Decision

`recipe` là identity/variant; `recipe_version` chứa ingredients/process/yield. Published version immutable. Nested recipes được phép với cycle check và max depth.

### Consequences

- Reproducibility tốt.
- Cần impact preview và dependency traversal.

## ADR-008 — Portion as contextual observation

**Status:** Accepted.

### Decision

Household measure conversion phụ thuộc food, unit, context, source, region và quality; không tạo conversion toàn cục kiểu “1 bát = X g”.

### Consequences

- Portion resolver phức tạp hơn nhưng đúng semantics.
- Không công bố percentile nếu không có sample study.

## ADR-009 — Deterministic calculation engine

**Status:** Accepted.

### Decision

Pure domain calculator; no network/DB/LLM. Inputs pin profile/recipe/portion/factors và engine version.

### Consequences

- Testability/replay cao.
- Application layer phải chuẩn bị đầy đủ context.

## ADR-010 — LLM constrained to extraction

**Status:** Accepted.

### Decision

LLM parse text thành structured mentions. Nó không quyết định nutrient facts hoặc tự do tạo canonical IDs. Candidate rerank chỉ xem xét sau gate.

### Consequences

- Giảm hallucination blast radius.
- Cần schema, semantic validation, provider abstraction và benchmark.

## ADR-011 — Lexical retrieval before embeddings

**Status:** Accepted.

### Decision

Exact alias → normalized alias → trigram/full-text → rule score. Embeddings chỉ thêm khi top-k recall slice cụ thể không đạt.

### Consequences

- Explainable và dễ debug.
- Cần alias/normalization tốt.

## ADR-012 — Analysis snapshot and append-only revisions

**Status:** Accepted.

### Decision

Mỗi analysis revision lưu interpretation, evidence, result và version context. Correction tạo revision mới; không overwrite.

### Consequences

- Storage tăng.
- Audit/replay/product trust tốt.

## ADR-013 — External provider through anti-corruption layer

**Status:** Accepted.

### Decision

Commercial/open external nutrition API có thể làm baseline, bootstrap, shadow hoặc explicit fallback. Response map vào internal DTO; provider fields không đi thẳng vào domain/database.

### Consequences

- Giảm lock-in.
- Cần provider comparison và disclosure.
- Không tự publish provider result thành canonical evidence.

## ADR-014 — Human curation required for publication

**Status:** Accepted.

### Decision

Automation được phép ingest, normalize, generate candidates và flag anomalies. Publish canonical mapping/recipe/profile/portion cần workflow và quyền phù hợp.

### Consequences

- Curation capacity là product constraint.
- Cần queue analytics, SLA và audit.

## ADR-015 — No Redis/Kafka/Kubernetes in MVP

**Status:** Accepted deferred infrastructure.

### Decision

Dùng PostgreSQL jobs/outbox, application memory cache có giới hạn, managed containers. Không thêm distributed components nếu chưa có metric.

### Reconsider trigger

- job throughput/replay requirement;
- cache pressure;
- platform policy;
- multi-service scaling.

## ADR-016 — VietnameseMealBench as release gate

**Status:** Accepted.

### Decision

Parser/resolver/portion/calculator behavior release phải có report trên benchmark versioned, slices và regression threshold.

### Consequences

- Dataset/annotation là source artifact quan trọng.
- Không ship model/prompt theo cảm giác.

## ADR-017 — One-question clarification strategy

**Status:** Accepted.

### Decision

Mỗi clarification turn hỏi một dimension có expected error reduction lớn nhất. Có answer options và free-text/other; tối đa turn theo product policy.

### Consequences

- Backend cần persisted state machine.
- Product analytics phải đo abandonment/success.

## ADR-018 — Source releases are staged, not auto-activated

**Status:** Accepted.

### Decision

Acquire → verify → parse → validate → map → report → approve → activate. Raw immutable; activation pointer có rollback.

### Consequences

- Upstream update không phá production âm thầm.
- Cần source adapter contract và impact reports.

## ADR-019 — API estimates expose quality, not fake probability

**Status:** Accepted.

### Decision

MVP trả `resolution_status`, `data_quality`, assumptions và bounded range. Không trả user-facing probability 0–1 cho tới khi calibrated.

### Consequences

- Product copy rõ hơn.
- Internal heuristic score vẫn được log/evaluate.

## ADR-020 — Documentation and behavior versioning

**Status:** Accepted.

### Decision

Blueprint, API, parser schema, resolution policy, source selection policy, calculator và catalog release có version độc lập. Release snapshot ghi tất cả versions.

### Consequences

- Tăng metadata nhưng hỗ trợ audit/replay.
- Thay đổi behavior cần changelog và compatibility review.

## ADR template cho quyết định mới

```markdown
# ADR-NNN — Title

Status: proposed
Date: YYYY-MM-DD
Owners:
Related requirements:

## Context

## Decision

## Alternatives considered

## Consequences

### Positive

### Negative

## Verification / fitness function

## Reconsider trigger

## Migration / rollback
```

## ADR governance

- Accepted ADR không sửa nội dung quyết định; tạo ADR superseding.
- Pull request ảnh hưởng invariant phải link ADR.
- Mỗi milestone review proposed/deferred ADR.
- Runtime/infrastructure decision phải có measurement artifact.
