# Review cuối và các quyết định kiến trúc đã chốt

**Phiên bản:** 1.0.0  
**Trạng thái:** Accepted baseline  
**Ngày review:** 2026-07-23  
**Review scope:** Product, domain, data, AI, runtime, open-source/market landscape, delivery sequencing

## 1. Executive assessment

Blueprint v1.0 được đánh giá là **development-ready với điều kiện triển khai theo walking skeleton và decision gates**. Core architecture không cần viết lại sau khi đối chiếu với các sản phẩm và repository tương tự. Điểm mạnh khác biệt nằm ở evidence/provenance, versioning, deterministic calculation và Vietnamese-specific curation.

Rủi ro lớn nhất không phải framework hay throughput. Ba sai số chi phối chất lượng là:

1. **Identity error:** resolve sai món, biến thể, trạng thái chế biến hoặc brand.
2. **Portion error:** quy đổi sai serving/household measure sang edible grams.
3. **Composition error:** chọn sai profile hoặc trộn evidence không tương thích.

Thứ tự ưu tiên bắt buộc:

```text
Product loop + domain semantics
→ source quality/provenance
→ deterministic calculation
→ evaluation/curation
→ API ergonomics
→ measured optimization
```

## 2. Market review: điều gì được xác nhận

Các hệ thống hiện có thường tối ưu một trong bốn hướng:

- Natural-language nutrition API.
- AI-first meal logging.
- Recipe/ingredient management.
- Food ontology hoặc benchmark.

Không repo công khai nào được xem là nền tảng thay thế trực tiếp cho toàn bộ domain core. Kết luận:

- Ý tưởng sản phẩm đã được thị trường xác nhận.
- Không cần phát minh lại UX meal logging hoặc ingredient parsing từ số 0.
- Không nên fork một recipe manager rồi biến nó thành nutrition evidence system.
- Phần cần tự sở hữu là Vietnamese catalog, resolution policy, portion evidence, recipe/composition versioning, calculation trace và quality workflow.

Chi tiết nằm trong [`15_OPEN_SOURCE_AND_MARKET_REFERENCE_STRATEGY.md`](15_OPEN_SOURCE_AND_MARKET_REFERENCE_STRATEGY.md).

## 3. Các quyết định được giữ nguyên

### 3.1 Modular monolith

Một codebase, một PostgreSQL database, module boundary rõ. Chỉ tách service khi có bằng chứng về:

- independent scaling;
- release cadence;
- ownership;
- fault isolation;
- regulatory/data boundary.

### 3.2 PostgreSQL-first

PostgreSQL đủ cho identity, taxonomy, recipe dependency, provenance, versioning, fuzzy search và snapshot. Không thêm graph/vector/search database theo trực giác.

### 3.3 Deterministic calculation

Calculator là pure domain code. Nó nhận input đã resolve và không gọi:

- LLM;
- network;
- database;
- clock ngầm;
- random generator.

Mọi version/factor phải được truyền hoặc ghi rõ trong calculation context.

### 3.4 LLM as constrained parser

LLM chỉ trích xuất claims từ text. Output qua bốn lớp:

```text
JSON schema validation
→ Rust DTO parsing
→ semantic validation
→ catalog/resolution policy
```

### 3.5 Unified food identity

`food_entity` là identity chung. `ingredient` là role trong recipe. Không tạo hai ontology song song rồi đồng bộ thủ công.

### 3.6 Evidence-based composition

Một food có thể có nhiều composition profiles. Selection policy chọn profile theo specificity, geography, state, quality, recency và derivation method.

### 3.7 Immutable publication và analysis revision

Published recipe/profile/portion không update-in-place. User correction tạo revision mới; lịch sử gốc được bảo toàn.

## 4. Những thay đổi quan trọng trong v1.0

### 4.1 Product scope trở thành contract

`02_PRODUCT_SCOPE_AND_REQUIREMENTS.md` định nghĩa persona, use case, non-goals, SLO, functional requirements và acceptance criteria. Engineering không được tự mở rộng scope bằng cách “tiện thể” thêm image/barcode/personalization.

### 4.2 Walking skeleton trước MVP

Phase đầu thu hẹp còn:

```text
20 foods + 10 dishes + 10 measures + 50–100 texts + 4 nutrients
```

Mục tiêu là chứng minh toàn bộ loop từ text đến correction và reproducibility, không phải đạt catalog coverage.

### 4.3 Product feedback loop là first-class

Clarification/correction không còn là phụ lục API. Nó có state machine, interaction budget và analytics riêng trong `17_CLARIFICATION_CORRECTION_UX_SPEC.md`.

### 4.4 VietnameseMealBench là release artifact

Mọi thay đổi parser, resolver, portion policy hoặc calculator phải được đánh giá trên benchmark end-to-end, theo slices tiếng Việt.

### 4.5 Build/buy/reuse có policy

External API có thể dùng cho baseline, bootstrap hoặc shadow mode, nhưng phải qua anti-corruption adapter. External provider không định hình canonical model.

### 4.6 Source adapters có contract

Dataset import phải hỗ trợ release discovery, checksum, immutable raw preservation, schema drift report, activation và rollback.

### 4.7 ADR được hoàn thiện

Các quyết định nền tảng có trạng thái, context, consequence, verification và trigger để reconsider.

## 5. Những phần bị loại khỏi MVP

| Hạng mục | Trạng thái | Điều kiện xem xét lại |
|---|---|---|
| Microservices | Deferred | Ownership/scaling/fault-isolation evidence |
| Redis | Deferred | Cache pressure đo được và invalidation model rõ |
| Kafka/NATS | Deferred | Multi-consumer replay/throughput requirement |
| Kubernetes | Deferred | Platform requirement hoặc vận hành vượt managed containers |
| Vector retrieval | Deferred | Lexical top-k recall không đạt gate |
| LLM reranker | Deferred | Offline + online uplift có ý nghĩa |
| Full FoodOn import | Rejected for MVP | Interoperability use case cụ thể |
| Portion percentiles | Deferred | Có sampling methodology và sample size |
| User-facing probability confidence | Deferred | Score được calibrated |
| Full micronutrients | Deferred | Source completeness và use case rõ |
| Image input | Deferred | Text loop đạt product-market signal |

## 6. Những điểm không được overengineer

- Không áp dụng CQRS/event sourcing cho toàn hệ thống; chỉ dùng immutable versions và outbox nơi cần.
- Không dùng arbitrary-precision decimal cho mọi phép tính nếu unit newtypes và rounding policy đã đủ.
- Không tạo taxonomy quá sâu trước khi có query/use case.
- Không tạo generic rules engine; policies là typed domain code.
- Không materialize mọi recipe/profile trước khi có performance data.
- Không xây self-hosted model pipeline trong MVP.
- Không xây admin UI hoàn hảo; ưu tiên review queue, merge, publish, diff và audit.

## 7. Quality attributes theo thứ tự

1. **Traceability** — result truy được evidence.
2. **Correctness** — invariant và unit-safe calculation.
3. **Reproducibility** — result cũ không đổi âm thầm.
4. **User recoverability** — hiểu và sửa interpretation dễ.
5. **Explainability** — biết vì sao resolve/chọn evidence.
6. **Modifiability** — source/model/runtime thay mà không phá domain.
7. **Privacy/security** — tối thiểu hóa dữ liệu và kiểm soát provider.
8. **Availability** — graceful degradation.
9. **Performance/cost** — đạt budget bằng measurement.

## 8. Architecture fitness functions

Các kiểm tra sau phải chạy tự động hoặc theo release gate:

| Fitness function | Cách kiểm chứng |
|---|---|
| Published version immutable | DB constraint + integration test |
| Recipe graph acyclic | publish-time recursive query test |
| Missing khác zero | schema/check + calculator fixtures |
| Domain không phụ thuộc framework | crate dependency check |
| Result reproducible | snapshot replay test |
| Unknown không bị ép match | benchmark slice |
| LLM không tạo nutrient | schema + semantic validation |
| No network in calculator | unit-test crate isolation |
| Import idempotent | repeated-release integration test |
| API correction append-only | revision/history test |

## 9. Các quyết định còn conditional

### Rust runtime

Chọn Rust với điều kiện Phase 0.5 chứng minh team ship an toàn. Runtime có thể chuyển sang Kotlin hoặc TypeScript mà không đổi domain/database semantics.

### External provider bootstrap

Có thể dùng để:

- tạo baseline;
- shadow compare;
- fallback có disclosure;
- hỗ trợ data gap có policy.

Không dùng provider response để tự động publish canonical evidence.

### Nested recipes

Được hỗ trợ với bounded depth và cycle check. Walking skeleton có thể chỉ dùng một cấp.

## 10. Decision gates trước beta

### Domain gate

- Glossary và invariant accepted.
- Recipe/profile/version semantics được test.
- Product owner hiểu `estimate`, `evidence`, `clarification`.

### Data gate

- Source/license registry đầy đủ.
- Seed catalog reviewed.
- Missing/zero/trace semantics không bị mất khi import.

### Intelligence gate

- VietnameseMealBench đạt mục tiêu.
- Unknown/ambiguity handling đạt precision target.
- Baseline comparison được lưu theo release.

### Product gate

- Median confirmation/correction interaction nằm trong budget.
- User hiểu assumption trong usability test.
- Không hiển thị false precision.

### Operations gate

- Backup/restore, rollback và provider outage drill pass.
- Cost, latency, error và quality dashboards hoạt động.

## 11. Final assessment

Blueprint v1.0 đủ để trở thành tài liệu xương sống vì nó trả lời được:

- Xây sản phẩm gì và không xây gì.
- Domain truth được biểu diễn thế nào.
- Database lưu identity, evidence và version ra sao.
- AI được phép và không được phép làm gì.
- Quality được đo bằng benchmark nào.
- User sửa lỗi bằng flow nào.
- Tái sử dụng thị trường ở đâu.
- Bắt đầu từ vertical slice nào.
- Khi nào được thêm hạ tầng hoặc thuật toán phức tạp.

Kiến trúc đích được giữ, nhưng sequencing đã được chỉnh để **không xây một knowledge platform hoàn hảo trước khi chứng minh một interaction hữu ích**.
