# Open-source và market reference strategy

**Phiên bản:** 1.0.0  
**Mục tiêu:** Tận dụng thị trường mà không làm mất quyền sở hữu domain core hoặc tạo license/lock-in risk

## 1. Kết luận chiến lược

Không có một repository nên được fork làm foundation duy nhất. Hệ thống cần chiến lược **selective reuse**:

```text
learn UX/schema/operations from existing systems
+ reuse libraries/adapters có license phù hợp
+ build canonical Vietnamese evidence core
+ benchmark against external APIs
```

## 2. Capability map

| Capability | Reference candidates | Strategy |
|---|---|---|
| Natural-language meal logging | NutriNutri, commercial APIs | UX/baseline reference |
| Ingredient quantity parsing | ingredient-parser | Evaluate/port schema/tooling |
| Recipe/unit administration | Mealie, Tandoor | Learn curation UX/data patterns |
| Meal diary/privacy | FoodYou | Product/storage reference |
| Food ontology | FoodOn | External mapping only |
| Nutrition benchmark | NutriBench | Baseline inspiration/data where license permits |
| Branded product data | Open Food Facts | Future source adapter |
| Nutrient composition | USDA FDC | Primary official source adapter |

## 3. Repository assessment rubric

Mỗi candidate được chấm theo:

1. Functional fit.
2. Domain semantic fit.
3. Language/runtime fit.
4. License compatibility.
5. Maintenance/activity.
6. Test maturity.
7. Security/supply-chain exposure.
8. Data provenance.
9. Internationalization/Vietnamese fit.
10. Cost to adapt versus rewrite.

Không dùng star count làm quyết định chính.

## 4. Candidate reviews

### 4.1 `strangetom/ingredient-parser`

**Giá trị:** structured ingredient schema, quantity/unit/name/preparation extraction, annotation/training/test concepts.

**Reuse decision:** Evaluate as offline baseline và annotation reference. Không đặt trực tiếp trên synchronous production path trước khi đo latency, language fit và service complexity.

**Gaps:** recipe-line context khác meal narrative; Vietnamese corpus cần tự xây; entity linking và nutrition ngoài scope.

### 4.2 Mealie

**Giá trị:** mature recipe management, food/unit stores, import/admin UX, REST patterns.

**Reuse decision:** UX/data-flow reference. Không fork backend vì domain, license và product boundary khác.

**Gaps:** recipe manager không phải evidence-based nutrition resolution engine; parsing đa ngôn ngữ vẫn cần domain-specific evaluation.

### 4.3 Tandoor Recipes

**Giá trị:** food/unit/amount separation, recipe editing, import/plugin ideas.

**Reuse decision:** Learn extensibility và curation patterns; evaluate source-plugin concepts.

**Gaps:** transactional recipe product, không giải quyết immutable nutrition evidence snapshots.

### 4.4 NutriNutri

**Giá trị:** AI meal logging UX, privacy-first positioning, correction/product flow inspiration.

**Reuse decision:** Product benchmark; không copy LLM-direct nutrition semantics.

**Gaps:** AI estimation may combine interpretation and nutrient generation, trái evidence-first architecture.

### 4.5 FoodYou

**Giá trị:** open/private meal diary, source integration và user-facing tracking.

**Reuse decision:** Learn diary/export/local privacy UX; evaluate data adapters.

**Gaps:** mobile product architecture khác backend knowledge core.

### 4.6 NutriBench

**Giá trị:** nutrition estimation benchmark and evaluation methodology.

**Reuse decision:** Compare metric design và, nếu license cho phép, dùng một phần làm external benchmark. Không dùng làm ground truth duy nhất.

**Gaps:** không đo đầy đủ food identity, portion evidence, clarification và provenance; VietnameseMealBench vẫn bắt buộc.

### 4.7 FoodOn

**Giá trị:** standardized food vocabulary/facets/external interoperability.

**Reuse decision:** Optional external IDs và mapping layer. Không import toàn bộ ontology vào MVP database.

**Gaps:** complexity, ontology reasoning và curation model không phù hợp request-path transactional needs.

## 5. Commercial/external provider strategy

External providers có thể được dùng theo bốn mode:

### Baseline mode

Chạy cùng benchmark để biết internal system có thực sự tốt hơn hay không.

### Bootstrap mode

Dùng provisional response trong prototype, có disclosure, trong lúc internal coverage còn nhỏ.

### Shadow mode

User nhận internal hoặc selected primary result; provider khác chạy async để so sánh, không ảnh hưởng response.

### Explicit fallback mode

Chỉ khi internal evidence thiếu và product policy cho phép. Result phải gắn `source_kind=external_provider`, không được masquerade thành curated internal evidence.

## 6. Anti-corruption layer

Mọi provider map về:

```text
ProviderAnalysisResult
- provider
- provider_model_or_api_version
- raw_reference
- parsed_items
- nutrient_values
- declared_units
- warnings
- terms/cache restrictions
- generated_at
```

Application chuyển nó thành comparison/baseline DTO. Không dùng provider food IDs làm canonical IDs. Không lưu/cache vượt điều khoản.

## 7. License and security gate

Trước khi reuse:

- record repository URL, commit/tag và license;
- identify copied code, model, weights, dataset và documentation licenses riêng;
- review copyleft/network-use obligations;
- run dependency/security scans;
- maintain attribution/NOTICE nếu cần;
- verify training data/model-use terms;
- document upgrade/removal path.

## 8. Build/buy decision template

```markdown
Capability:
Problem metric:
Current baseline:
Candidate solution:
Alternatives:
License/terms:
Data/privacy impact:
Integration cost:
Operational cost:
Exit/replace plan:
Decision:
Review date:
```

## 9. What must remain proprietary/internal domain ownership

Dù project có open-source hay commercial model, hệ thống phải tự sở hữu:

- canonical Vietnamese food identity;
- aliases, regional distinctions và modifier semantics;
- portion evidence/measurement policy;
- source mapping/quality grades;
- recipe/composition selection;
- deterministic calculator;
- clarification/correction state;
- analysis snapshot/replay;
- VietnameseMealBench;
- curation workflow.

## 10. Review cadence

- Quarterly hoặc trước major build/buy decision.
- Không cập nhật chỉ vì star/version thay đổi.
- Mọi adoption phải có ADR và pinned reference.
