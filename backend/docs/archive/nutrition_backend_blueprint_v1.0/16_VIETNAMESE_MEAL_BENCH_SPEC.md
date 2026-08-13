# VietnameseMealBench specification

**Phiên bản:** 1.0.0  
**Mục tiêu:** Benchmark end-to-end cho text bữa ăn tiếng Việt, không chỉ đánh giá LLM calories

## 1. Benchmark philosophy

Benchmark phải tách các lỗi:

```text
mention extraction
→ canonical resolution
→ portion resolution
→ evidence selection
→ deterministic calculation
→ decision to clarify/resolve/insufficient
```

Một estimate đúng ngẫu nhiên do hai lỗi bù nhau không được coi là pipeline đúng.

## 2. Unit of evaluation

Mỗi sample gồm:

```json
{
  "sample_id": "vmb-000001",
  "text": "trưa ăn bát cơm, ít thịt kho, bỏ phần mỡ",
  "locale": "vi-VN",
  "context": {},
  "mentions": [],
  "negations": [],
  "modifiers": [],
  "acceptable_food_ids": [],
  "portion_reference": [],
  "expected_decision": "clarify|resolve|insufficient",
  "acceptable_nutrition": {},
  "tags": [],
  "annotation_evidence": [],
  "adjudication_status": "approved"
}
```

## 3. Annotation layers

### Layer A — Text spans

- food mention;
- quantity;
- unit;
- preparation;
- modifier;
- brand;
- negation;
- consumed fraction;
- temporal/non-food text.

### Layer B — Semantic interpretation

- candidate canonical IDs;
- acceptable equivalent IDs;
- required clarification dimension;
- regional ambiguity;
- explicit versus inferred facts.

### Layer C — Portion evidence

- explicit grams;
- household measure observation;
- count weight;
- acceptable range;
- evidence source and quality;
- whether clarification is required.

### Layer D — Nutrition result

- expected profile/recipe version;
- expected deterministic result;
- tolerance/range;
- missing nutrients;
- assumptions.

## 4. Required slices

- Có dấu / không dấu.
- Telex-like typo và common misspelling.
- Vietnamese-English code switching.
- Northern/Central/Southern names.
- Brand/product names.
- Colloquial/slang.
- Household units: bát, chén, tô, vá, muỗng, nắm, miếng, phần.
- Approximation: khoảng, hơn nửa, một chút, ít.
- Negation: không ăn, bỏ, không uống hết.
- Partial consumption: ăn nửa, để lại nước, bỏ da/mỡ.
- Multiple items.
- Long narrative/noise.
- Unknown food.
- Similar foods with material nutrition difference.
- Composite dish/modifiers.

## 5. Dataset splits

```text
train/dev        cho prompt/rule development
public-test      regression thường xuyên
sealed-test      release gate, access hạn chế
challenge        production-derived hard cases đã de-identify
```

Không tune trực tiếp trên sealed-test. Mọi sample production phải qua privacy review và de-identification.

## 6. Annotation protocol

1. Annotator A và B annotate độc lập.
2. Disagreement được adjudicate bởi domain reviewer.
3. Ambiguous case có thể có nhiều acceptable answers.
4. Portion ground truth ghi method: weighed, label, curated observation, expert estimate.
5. Không gắn “ground truth exact kcal” khi portion không exact; dùng acceptable bound.
6. Version dataset và annotation guideline cùng nhau.

## 7. Metrics

### Parser

- Mention span precision/recall/F1.
- Quantity exact/normalized accuracy.
- Unit accuracy.
- Modifier/negation F1.
- Schema-valid rate.

### Resolver

- Top-1 accuracy.
- Top-3 recall.
- Mean reciprocal rank.
- Unknown detection precision/recall.
- Over-resolution rate.
- Clarification decision precision/recall.

### Portion

- Gram MAE/median absolute error.
- Relative error where valid.
- Bound coverage.
- Unit-to-food compatibility accuracy.

### Nutrition

- Energy MAE and median absolute percentage error, chỉ trên suitable subset.
- Macro MAE.
- Acceptable-bound hit rate.
- Completeness-aware scoring.
- Replay equivalence.

### Product decision

- Finalized-without-correction rate.
- Clarification turns.
- Clarification answerability.
- User correction rate in controlled study.

## 8. Baseline matrix

Mỗi release nên so sánh:

1. Rule-only parser/resolver.
2. LLM direct nutrition estimation — research baseline, không production design.
3. LLM extraction + lexical catalog.
4. Hybrid internal pipeline.
5. External provider(s), nếu terms cho phép.
6. Previous production release.

## 9. Initial release gates

Các target dưới đây là starting gates, được điều chỉnh bằng ADR sau measurement:

| Metric | Walking skeleton | MVP beta |
|---|---:|---:|
| Schema-valid parser | ≥ 99% | ≥ 99.5% |
| Mention F1 | ≥ 0.90 | ≥ 0.94 |
| Known food top-3 recall | ≥ 0.90 | ≥ 0.96 |
| Known food top-1 accuracy | ≥ 0.78 | ≥ 0.88 |
| Unknown detection precision | ≥ 0.85 | ≥ 0.92 |
| Over-resolution rate | ≤ 8% | ≤ 3% |
| Calculation fixture pass | 100% | 100% |
| Replay pass | 100% | 100% |

Portion/nutrition target chỉ freeze sau khi có measured subset đủ chất lượng.

## 10. Regression policy

Release bị block nếu:

- calculator fixture fail;
- schema-valid giảm dưới gate;
- critical slice giảm vượt agreed tolerance;
- unknown over-resolution tăng material;
- safety/product decision regression;
- result không replay được.

Aggregate metric không được che slice regression nghiêm trọng.

## 11. Dataset governance

- Store manifest/checksum/license/consent.
- Sample IDs stable; edits create new dataset version.
- Personal text de-identified.
- Access control cho sealed set.
- Annotation tool/export format documented.
- Changelog ghi samples added/removed/corrected.

## 12. Release report template

```markdown
Benchmark version:
System behavior versions:
Data coverage:
Baseline comparison:
Aggregate metrics:
Slice metrics:
Regressions:
Known limitations:
Human review summary:
Release decision:
```
