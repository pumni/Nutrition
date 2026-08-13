# Nutrition calculation specification

**Phiên bản:** 1.0.0  
**Trạng thái:** Normative calculation behavior  
**Owner:** Domain/calculation owner


## 1. Mục tiêu

Đặc tả này định nghĩa phép tính chuẩn, thứ tự xử lý, semantics của missing data và điều kiện hệ thống được phép trả kết quả. Calculation engine phải deterministic, versioned và không phụ thuộc LLM/network.

## 2. Canonical units

| Dimension | Canonical unit |
|---|---|
| Mass | gram (`g`) |
| Volume | millilitre (`ml`) |
| Energy | kilocalorie (`kcal`) |
| Macronutrient | gram (`g`) |
| Micronutrient | milligram hoặc microgram theo nutrient definition |

Mọi conversion phải dùng unit registry. Không parse unit string trong calculation engine.

## 3. Calculation input DTO

```rust
struct CalculationInput {
    engine_version: String,
    items: Vec<ResolvedItemInput>,
}

struct ResolvedItemInput {
    food_id: FoodId,
    mass: MassEstimate,
    composition: CompositionSnapshot,
    recipe: Option<RecipeCalculationInput>,
}

struct MassEstimate {
    central_g: f64,
    lower_g: Option<f64>,
    upper_g: Option<f64>,
    evidence_id: Option<PortionObservationId>,
    method: MassResolutionMethod,
}
```

DTO phải chứa snapshot values cần tính, không buộc calculator query database.

## 4. Direct food calculation

Với profile basis `B` gram và edible mass `M`:

```text
result_nutrient = profile_nutrient × M / B
```

Với profile chuẩn per 100 g:

```text
N_item = N_100g × M_g / 100
```

Nếu profile theo serving, chỉ convert sang mass basis khi serving profile và portion semantics tương thích. Không suy ra per-100g từ serving nếu thiếu serving weight.

## 5. Edible portion

Nếu người dùng nhập gross weight và food có edible fraction `E`:

```text
edible_mass = gross_mass × E
```

Nếu input đã biểu thị edible part, không áp dụng lần hai. `mass_basis` phải là:

- `gross_as_purchased`.
- `edible_raw`.
- `edible_cooked`.

Không mặc định gross = edible.

## 6. Volume to mass

```text
mass_g = volume_ml × density_g_per_ml
```

Density selection order:

1. Exact food + state + temperature/context.
2. Exact food generic density.
3. Curated food-group density.
4. Không convert; hỏi lại hoặc đánh dấu insufficient.

Không dùng density của nước cho mọi chất lỏng.

## 7. Count/household measure to mass

```text
mass_g = measure_amount × observation_gram_weight / observation_measure_amount
```

Resolver, không phải calculator, chọn observation. Calculator chỉ áp dụng evidence đã chọn.

## 8. Recipe calculation workflow

### 8.1 Required inputs

- Published recipe version.
- Component foods.
- Component edible raw/cooked masses.
- Component composition profiles.
- Final cooked output mass hoặc approved yield policy.
- Optional retention/fat/salt uptake factors.

### 8.2 Base algorithm

Cho component `i`, nutrient `n`:

```text
raw_contribution(i,n)
  = composition(i,n) × edible_mass(i) / profile_basis(i)
```

Nếu có nutrient retention factor phù hợp:

```text
retained_contribution(i,n)
  = raw_contribution(i,n) × retention_factor(i,n,process)
```

Nếu không có factor:

```text
retained_contribution = raw_contribution
warning = retention_factor_not_applied
```

Tổng recipe:

```text
total_recipe_nutrient(n)
  = Σ retained_contribution(i,n)
```

Theo 100 g cooked output:

```text
recipe_nutrient_per_100g(n)
  = total_recipe_nutrient(n) × 100 / cooked_output_mass_g
```

Theo serving:

```text
serving_mass_g = cooked_output_mass_g / serving_count
serving_nutrient(n) = total_recipe_nutrient(n) / serving_count
```

### 8.3 Yield

Nếu có measured cooked output weight, dùng trực tiếp.

Nếu chỉ có raw total weight và yield factor `Y`:

```text
cooked_output_mass = raw_total_weight × Y
```

Yield factor không tự động thay nutrient total; nó chủ yếu thay denominator/concentration, trừ khi domain factor đặc tả uptake/loss riêng.

### 8.4 Water/fat/salt uptake/loss

Không mô hình hóa tất cả thay đổi chỉ bằng một generic retention factor.

- Nước thay đổi cooked mass và water nutrient.
- Dầu hấp thụ phải là component hoặc uptake factor có evidence.
- Dầu chiên không được tính toàn bộ lượng dầu ban đầu nếu phần lớn còn lại.
- Muối trong nước luộc có thể cần salt uptake factor.
- Draining có thể loại một phần liquid/fat/nutrient.

MVP policy:

1. Ưu tiên recipe components phản ánh lượng thực sự vào final dish.
2. Dùng measured final weight.
3. Chỉ áp dụng specialized factors đã curated.
4. Nếu không có, tạo warning thay vì sáng tạo factor.

## 9. Nested recipes

Calculator resolve nested recipe thành composition snapshot trước khi dùng làm parent component.

Rules:

- Max depth mặc định 5.
- Cycle bị cấm khi publish.
- Mỗi nested node giữ calculation trace riêng.
- Cache calculated profile theo `(recipe_version_id, engine_version, factor_dataset_version)`.
- Parent không cần biết raw ingredients sâu hơn nếu child snapshot hợp lệ, nhưng trace phải liên kết được.

## 10. Composition profile selection contract

Calculator không tự chọn profile. Selection service trả:

```json
{
  "profile_id": "...",
  "basis_amount": 100,
  "basis_unit": "g",
  "values": {},
  "quality_grade": "B",
  "selection_reason": "curated_local_direct_match"
}
```

Nếu profile thiếu một nutrient:

- Item result nutrient = missing.
- Total có thể tính partial sum nhưng phải trả `completeness_ratio`.
- Không coi missing là zero.

## 11. Energy policy

### 11.1 Sources

Energy có thể là:

- Direct measured/declared value.
- Source-calculated value.
- Internal calculation từ energy-contributing components.

### 11.2 Precedence

MVP đề xuất:

1. Exact branded label declared energy cho exact product.
2. Published direct profile energy.
3. Recipe-calculated energy từ selected component profiles.
4. Internal macronutrient formula khi policy cho phép.

Không overwrite declared energy bằng macro formula. Có thể tính `derived_energy_for_validation` và ghi anomaly nếu chênh lệch vượt threshold.

### 11.3 Factor registry

Không hard-code một formula duy nhất trong nhiều nơi. Dùng `energy_factor_policy_version` và nutrient-specific factors theo domain/source convention.

## 12. Missing, trace, not detected và zero

| Status | Meaning | Arithmetic behavior |
|---|---|---|
| measured/declared/calculated | Numeric known | Include |
| trace | Có nhưng dưới reporting threshold | Theo policy: 0 cho sum display hoặc bounded trace; giữ status |
| not_detected | Dưới detection limit | Không tự đồng nhất với zero; policy-specific |
| missing | Không có dữ liệu | Exclude và giảm completeness |
| zero | Numeric zero có evidence | Include zero |

API phải giữ status cho nutrient quan trọng khi client yêu cầu evidence mode.

## 13. Completeness

Cho total nutrient `n`:

```text
completeness_ratio(n)
 = mass_of_items_with_known_nutrient(n)
   / total_resolved_mass
```

Hoặc item-weighted policy nếu mass không phù hợp. Policy phải versioned.

Không hiển thị total như đầy đủ nếu completeness thấp hơn threshold. Ví dụ:

- ≥ 0,9: usable.
- 0,6–0,9: partial warning.
- < 0,6: insufficient.

Các ngưỡng phải được product/domain review.

## 14. Uncertainty propagation

### MVP bounded approach

Nếu nutrient concentration coi cố định và mass có lower/upper:

```text
N_lower = C × M_lower / basis
N_upper = C × M_upper / basis
```

Với nhiều items không biết correlation:

```text
total_lower = Σ item_lower
total_upper = Σ item_upper
```

Đây là conservative bounded sum, không phải statistical confidence interval.

Nếu composition profile cũng có min/max:

```text
lower = min_product(nonnegative C_range, M_range) / basis
upper = max_product(...)
```

Nutrients/mass không âm nên đơn giản:

```text
lower = C_lower × M_lower / basis
upper = C_upper × M_upper / basis
```

Không gọi bounds là p10/p90 nếu không có statistical method.

## 15. Rounding

- Internal calculation giữ precision đầy đủ hợp lý.
- Không round từng ingredient trước khi sum.
- Persistence có normalized precision.
- Presentation rounding theo nutrient/unit và magnitude.
- Range rounding phải giữ `lower ≤ central ≤ upper` sau rounding.
- Declared label raw value giữ nguyên significant figures.

Ví dụ display:

- kcal: integer.
- macros ≥ 10 g: 1 decimal hoặc integer theo UX.
- macros < 10 g: 1 decimal.
- mg: domain-specific.

## 16. Validation rules

Calculator reject input khi:

- Negative mass/nutrient.
- Basis ≤ 0.
- Unit không canonical/convertible.
- Recipe output mass ≤ 0.
- Duplicate nutrient trong snapshot.
- Cycle marker/depth exceeded.
- NaN/infinity.

Calculator warning khi:

- Missing nutrient.
- Low-quality profile.
- Yield inferred.
- Retention factor absent.
- Portion bounds wide.
- Profile geographically weak.

## 17. Calculation trace

Trace không cần lộ toàn bộ cho client nhưng phải lưu đủ audit:

```json
{
  "engine_version": "calc-1.0.0",
  "item_mass_g": 550,
  "profile": {
    "id": "profile_x",
    "basis_g": 100
  },
  "operations": [
    {
      "nutrient": "energy_kcal",
      "source_amount": 92.4,
      "formula": "92.4 * 550 / 100",
      "result": 508.2
    }
  ]
}
```

Production trace có thể lưu structured operands thay formula string để giảm storage.

## 18. Versioning

Tăng engine version khi thay:

- Formula.
- Rounding affecting persisted result.
- Missing-data policy.
- Factor application order.
- Energy precedence.
- Uncertainty propagation.

Không cần tăng engine version khi chỉ sửa code không đổi output semantics, nhưng release vẫn được trace bằng build version.

## 19. Required test fixtures

1. Direct profile per 100 g.
2. Serving profile có serving weight.
3. Volume + exact density.
4. Count + portion observation.
5. Gross weight + edible fraction.
6. Recipe measured cooked weight.
7. Recipe inferred yield.
8. Nested recipe.
9. Missing nutrient.
10. Trace/not_detected.
11. Mass and concentration bounds.
12. Large/small rounding.
13. Invalid unit/basis.
14. Cycle/depth guard.
15. Reproducibility từ persisted snapshot.

## 20. v1.0 decision table for calculation outcomes

| Condition | Behavior |
|---|---|
| Explicit gram + compatible profile | Calculate direct |
| Contextual portion observation | Convert with evidence and quality |
| Multiple plausible portion values | Return bounded estimate or clarify |
| No valid portion evidence | `insufficient_portion_evidence` |
| Nutrient missing in selected profile | Preserve missing, do not zero-fill |
| Recipe component unresolved | Block published calculation |
| Declared label energy available | Follow energy precedence policy |
| Retention/yield absent | Do not invent factor; use documented fallback or flag |

## 21. Calculation context contract

Every run receives:

```text
calculation_engine_version
energy_policy_version
rounding_policy_version
profile snapshots
recipe snapshot
portion evidence
factor registry snapshot
requested nutrient set
```

System clock/current catalog is not an implicit input.

## 22. Uncertainty terminology

- `bounded_estimate`: deterministic min/typical/max từ evidence bounds.
- `measurement_interval`: range từ measured sample protocol.
- `confidence_interval`: chỉ dùng khi statistical methodology hợp lệ.
- `quality_grade`: đánh giá evidence, không phải xác suất correctness.

Không gọi heuristic bounds là confidence interval.

## 23. Recalculation policy

- User correction → new analysis revision.
- New catalog release không tự thay result cũ.
- Optional “updated estimate” tạo revision/recalculation record với new versions.
- Batch correction do calculator bug phải có migration/change record và preserve original.

## 24. Release gate

Calculation release bị block nếu:

- fixture hoặc property test fail;
- missing/zero semantics thay ngoài spec;
- rounding delta không documented;
- replay không tương đương;
- material delta không có impact report/domain approval.
