# Release và change management

**Phiên bản:** 1.0.0  
**Mục tiêu:** Thay đổi code, model, policy và data mà vẫn audit/replay/rollback được

## 1. Version dimensions

Một system release không chỉ có application version. Analysis context phải ghi:

```text
api_version
application_version
parser_schema_version
prompt_version
model/provider_version
normalization_version
resolution_policy_version
portion_policy_version
composition_selection_policy_version
calculation_engine_version
catalog_release
source_release_ids
```

## 2. Semantic versioning policy

### API

- Major: breaking contract/semantics.
- Minor: additive backward-compatible fields/endpoints.
- Patch: bug fix không đổi intended contract.

### Calculator

- Major: thay interpretation/formula/basis đáng kể.
- Minor: thêm nutrient/capability không đổi existing fixture semantics.
- Patch: sửa implementation để khớp spec; phải ghi impact.

### Parser/resolution behavior

Dùng behavior release version. Prompt/model/rules thay đổi dù schema không đổi vẫn cần release report.

### Catalog/data

Calendar hoặc semantic data release; immutable manifest/checksum và change summary.

## 3. Release bundle

Mỗi release bundle chứa:

- application artifacts/image digest;
- database migration range;
- behavior versions;
- catalog/source manifests;
- benchmark report;
- security/dependency report;
- known limitations;
- rollback plan;
- operator approval.

## 4. Change classes

| Class | Ví dụ | Required gate |
|---|---|---|
| Code-internal | refactor | CI + no behavior regression |
| API additive | new optional field | contract tests |
| Behavior | prompt/ranking threshold | VietnameseMealBench |
| Calculation | yield/energy policy | domain review + fixtures + replay impact |
| Data | new source/catalog release | validation + impact + approval |
| Schema | migration | migration/rollback/restore test |
| Security/privacy | retention/provider | security/legal/product review |

## 5. Database migration strategy

- Forward-only migrations.
- Expand → backfill/migrate → switch reads/writes → contract.
- Không rename/drop cột cùng release khi old app còn chạy.
- Long backfill chạy job có checkpoint.
- Migration không gọi external service.
- Schema version compatible với rolling deployment.

## 6. Data release activation

```text
staged
→ validation_passed
→ review_approved
→ active
→ superseded/rolled_back
```

Activation có impact summary:

- foods/profiles/portions added/changed;
- mapping changes;
- affected active queries/materializations;
- sample estimate deltas;
- benchmark delta.

## 7. Behavior release

Mọi parser/resolver release phải ghi:

- exact provider/model/config;
- prompt/rules checksum;
- benchmark version;
- aggregate và slice metrics;
- cost/latency;
- regressions/accepted tradeoffs;
- rollout percentage.

## 8. Rollout

Khuyến nghị:

```text
offline evaluation
→ staging replay
→ shadow mode
→ internal users
→ canary percentage
→ full rollout
```

Không dùng silent full rollout cho behavior hoặc data selection change lớn.

## 9. Rollback

### Application

Deploy previous image nếu schema compatible.

### Behavior

Switch active behavior registry pointer.

### Catalog/source

Switch active release pointer; không xóa release mới.

### Calculator

Rollback code/policy cho request mới; existing analysis giữ pinned version. Nếu cần recalculate, tạo revision/migration record.

## 10. Compatibility

- Consumers phải ignore unknown additive fields.
- Enum additions cần design tránh breaking generated clients.
- Public errors có stable codes.
- Deprecated field có timeline và telemetry.
- Internal DTO không được lộ nguyên dạng thành public API.

## 11. Documentation as code

- Docs nằm trong repository hoặc release artifact.
- PR thay behavior phải update spec/ADR/changelog.
- Broken-link/fence/empty-file checks trong CI.
- Manifest ghi file checksum.
- Major blueprint release được tag.

## 12. Incident-driven changes

Hotfix vẫn phải:

- có incident/change ID;
- test tối thiểu;
- ghi affected versions;
- post-release benchmark/backfill review;
- cập nhật ADR/spec nếu behavior thay đổi.

## 13. Release checklist

- [ ] CI/contract/calculation tests pass.
- [ ] Benchmark report accepted.
- [ ] Migration compatibility checked.
- [ ] Data/source impact reviewed.
- [ ] Security scans pass.
- [ ] Rollback target verified.
- [ ] Dashboards/alerts ready.
- [ ] Changelog and operator notes published.
- [ ] Release manifest/checksums generated.
