# Product scope và requirements

**Phiên bản:** 1.0.0  
**Trạng thái:** Accepted for implementation  
**Owner:** Product owner + tech lead  
**Mục tiêu:** Định nghĩa rõ hệ thống phải làm gì, không làm gì và điều kiện chấp nhận

## 1. Product statement

Backend cho phép người dùng mô tả bữa ăn bằng tiếng Việt tự nhiên và nhận một **ước tính dinh dưỡng có thể giải thích, sửa và truy nguồn**.

Giá trị cốt lõi:

- nhập nhanh hơn tìm từng món thủ công;
- hỗ trợ món Việt và cách nói địa phương;
- không che giấu ambiguity;
- không để LLM tự bịa calories;
- correction của người dùng trở thành tín hiệu chất lượng có kiểm soát.

## 2. Primary personas

### 2.1 Người dùng phổ thông

Muốn ghi bữa ăn nhanh, xem calories/macros gần đúng và sửa kết quả khi hệ thống hiểu sai.

### 2.2 Người theo dõi dinh dưỡng có hệ thống

Quan tâm lịch sử, assumptions, serving size và consistency giữa các lần ghi.

### 2.3 Curator/domain reviewer

Quản lý canonical foods, aliases, recipes, portions, composition evidence và source mappings.

### 2.4 Engineering/data operator

Import dataset, theo dõi quality, rollback release, điều tra regression và cost.

## 3. Jobs to be done

1. “Khi vừa ăn xong, tôi muốn mô tả bữa ăn bằng một câu để ghi lại trong vài giây.”
2. “Khi hệ thống không chắc, tôi muốn nó hỏi đúng một câu quan trọng thay vì đoán.”
3. “Khi kết quả sai, tôi muốn sửa món hoặc khẩu phần mà không nhập lại toàn bộ.”
4. “Khi xem calories, tôi muốn biết đây là estimate và assumption chính là gì.”
5. “Khi dữ liệu được cập nhật, tôi muốn lịch sử cũ không âm thầm thay đổi.”
6. “Khi curator sửa catalog, tôi muốn biết những analysis nào bị ảnh hưởng.”

## 4. Primary use cases

### UC-01 Analyze a meal text

**Input:** text, locale, optional timestamp/context.  
**Output:** completed estimate, clarification request hoặc insufficient-evidence result.

### UC-02 Answer clarification

User chọn một candidate hoặc cung cấp portion/context còn thiếu. Hệ thống tiếp tục analysis từ state đã lưu.

### UC-03 Correct an item

User thay food, quantity, unit, grams hoặc modifier. Hệ thống tạo analysis revision mới.

### UC-04 View estimate details

Trả item-level result, total, assumptions, evidence quality, provenance summary và revision history.

### UC-05 Curate canonical data

Curator tạo/merge/deprecate food, quản lý aliases, map source records, publish recipe/profile/portion versions.

### UC-06 Import a dataset release

Operator acquire, verify, parse, validate, stage, review, activate hoặc rollback source release.

## 5. Functional requirements

### FR-001 Input acceptance

- Chấp nhận Unicode UTF-8 tiếng Việt.
- Tối đa 2.000 ký tự và 10 food mentions trong MVP.
- Bảo toàn raw input trong phạm vi retention policy.
- Chuẩn hóa mà không phá text gốc.

### FR-002 Structured extraction

Parser phải trích xuất:

- source span/text;
- food phrase;
- quantity expression;
- unit expression;
- preparation/modifier;
- brand nếu được nói rõ;
- negation/partial-consumption;
- parser warnings.

Parser không được trả nutrient hoặc canonical ID chưa được cung cấp trong constrained candidate task.

### FR-003 Food resolution

- Exact/normalized alias trước fuzzy retrieval.
- Top candidates có features/evidence.
- Không ép match khi score/ambiguity không đạt policy.
- Locale, region, preparation và unit compatibility được dùng trong ranking.

### FR-004 Portion resolution

- Explicit grams ưu tiên cao nhất.
- Household/count/volume phải dùng food-context observation.
- Không có conversion hợp lệ → hỏi lại hoặc insufficient evidence.
- Personal prior chỉ dùng ở phase sau và phải disclosure.

### FR-005 Composition selection

- Chọn một profile theo policy version.
- Không trộn nutrient tùy tiện từ nhiều profile.
- Compiled profile phải lưu derivation trace.
- Missing không được đổi thành zero.

### FR-006 Deterministic calculation

- Calculator không gọi network/LLM/database.
- Hỗ trợ direct food và recipe-derived profile.
- Lưu calculation trace và engine version.
- Rounding chỉ ở output boundary.

### FR-007 Result semantics

Mỗi result phải có:

- estimated value;
- optional bounded range;
- assumptions;
- evidence/data-quality summary;
- item resolution status;
- revision/version identifiers;
- `is_estimate=true`.

### FR-008 Clarification

- Mỗi turn hỏi tối đa một dimension.
- Ưu tiên câu hỏi có expected error reduction cao nhất.
- Có phương án “không chắc/khác”.
- Không bắt user trả lời nếu result đủ hữu ích theo policy.

### FR-009 Correction

- Correction append-only.
- Tạo revision mới, không overwrite result cũ.
- Recalculate deterministic.
- Ghi source của correction: user/curator/system migration.

### FR-010 Curation

- Draft → validate → review → publish → deprecate.
- Merge cần redirect và impact preview.
- Published version immutable.
- Audit actor/time/reason.

### FR-011 Data import

- Release/checksum/schema version được ghi.
- Raw artifact immutable.
- Import idempotent.
- Không auto-activate release mới.
- Có rollback target.

### FR-012 Reproducibility

Một analysis revision phải pin hoặc snapshot đủ:

- parser/model/prompt/schema version;
- food IDs và candidate decision;
- portion observation/version;
- composition profile/version;
- recipe version;
- policy versions;
- calculation engine version.

## 6. Non-functional requirements

### NFR-001 Correctness

- Calculator fixtures: 100% pass.
- Published data invariant violations: 0.
- Unknown-food force-match rate nằm dưới release threshold.

### NFR-002 Availability

MVP target:

- Monthly availability target: 99.5% cho API core.
- LLM outage phải trả retryable/degraded response, không trả estimate bịa.
- Data import outage không ảnh hưởng read/analyze hiện tại.

### NFR-003 Latency

Initial budget, được hiệu chỉnh sau spike:

| Flow | Target |
|---|---:|
| Completed analysis p50 | ≤ 1.8 s |
| Completed analysis p95 | ≤ 4.0 s |
| Exact catalog search p95 | ≤ 150 ms |
| Correction recalculation p95 | ≤ 800 ms nếu không gọi parser |
| Admin publish transaction p95 | ≤ 1.5 s |

### NFR-004 Cost

- Theo dõi cost/request theo provider/model.
- Cache parse chỉ khi privacy và version key cho phép.
- Có monthly budget và alert; con số cụ thể do product finance chốt.

### NFR-005 Security/privacy

- TLS in transit, managed encryption at rest.
- Không log raw meal text mặc định.
- Provider payload tối thiểu hóa và không chứa user identity nếu không cần.
- User có export/delete theo policy.

### NFR-006 Maintainability

- Domain crate không import framework/database SDK.
- Migrations forward-only; destructive change có expand/migrate/contract.
- Public API có versioning.
- Mọi policy behavior có version.

### NFR-007 Observability

- Trace xuyên suốt parser → resolver → calculator → persistence.
- Metrics cho latency, cost, schema failure, unresolved, correction và data quality.
- Logs structured và redacted.

## 7. Walking skeleton scope

### Included

- 20 basic foods, 10 dishes, 10 measures.
- 50–100 labeled texts.
- 4 nutrients: energy, protein, carbohydrate, fat.
- Direct food + recipe một cấp.
- Text parse, exact/trigram resolution, one clarification flow.
- Correction, revision, calculation trace.
- One source adapter, one LLM provider.
- CLI hoặc minimal internal UI.

### Exit acceptance

- 20 golden end-to-end fixtures pass.
- Replay result byte/semantic-equivalent theo rounding policy.
- Unknown item tạo unresolved/clarification, không match tùy tiện.
- Correction không mất revision cũ.
- Team vận hành local stack từ README trong một buổi onboarding.

## 8. MVP beta scope

### Included

- 100–200 basic foods.
- 50–100 curated Vietnamese dishes.
- 20–30 measures.
- 300–500 initial texts; mở rộng theo production errors.
- Energy/macros; fiber/sodium khi đủ evidence.
- Recipe version, nested depth giới hạn.
- Curation review queue.
- Source release activation/rollback.
- VietnameseMealBench release report.

### Explicitly excluded

- Medical recommendations.
- Eating-plan prescription.
- Image understanding.
- Personalization model.
- Vector database.
- Restaurant-wide menu ingestion.
- Full barcode and packaged-product UX.
- Automatic web recipe scraping into published catalog.

## 9. Decision policy: resolve, clarify, insufficient

### Resolve

Khi identity và portion evidence đạt threshold, ambiguity không material đối với nutrition result.

### Clarify

Khi một câu hỏi ngắn có thể giảm đáng kể expected error hoặc chọn giữa candidates khác biệt lớn.

### Insufficient evidence

Khi không có conversion/profile đáng tin và clarification không thể giải quyết hợp lý.

Không dùng con số mặc định ẩn để tránh trạng thái insufficient.

## 10. Product safety requirements

- Luôn gọi là “ước tính”.
- Không khẳng định đo chính xác lượng người dùng đã hấp thụ.
- Không chẩn đoán bệnh hoặc đưa phác đồ.
- Không khuyến khích restriction cực đoan, bù trừ hoặc hành vi có hại.
- Với dữ liệu có uncertainty cao, UI ưu tiên range/assumption hơn chữ số thập phân.

## 11. Analytics events

Tối thiểu:

```text
analysis_submitted
analysis_completed
analysis_needs_clarification
clarification_answered
analysis_insufficient
item_corrected
analysis_confirmed
assumption_opened
source_details_opened
```

Không gửi raw meal text vào product analytics. Event phải dùng IDs/category an toàn.

## 12. Product KPIs

### Quality

- Confirmation-without-correction rate.
- Correction rate theo food/portion slice.
- Unresolved rate.
- Clarification success rate.
- Repeat correction rate.

### Experience

- Time to finalized log.
- Number of clarification turns.
- Abandonment after clarification.
- Percentage users mở assumptions.

### Operations

- Cost per finalized analysis.
- Curator queue age.
- Source freshness.
- Analysis replay success.

## 13. Acceptance criteria cho beta

- VietnameseMealBench gates trong file 16 đạt.
- P95 latency và availability đạt trong 14 ngày staging/beta.
- ≥ 90% completed flows cần tối đa một clarification turn trên controlled usability set.
- Không có critical/high unresolved security issue.
- Backup/restore và source rollback drill pass.
- Product copy được domain/safety reviewer duyệt.
- Curation staffing và SLA nội bộ được chốt.

## 14. Requirements traceability

Mỗi epic/ticket phải tham chiếu ít nhất một requirement ID. Test case quan trọng phải ghi `FR-*` hoặc `NFR-*`. ADR phải ghi requirements bị tác động. Release note phải nêu requirement behavior nào thay đổi.
