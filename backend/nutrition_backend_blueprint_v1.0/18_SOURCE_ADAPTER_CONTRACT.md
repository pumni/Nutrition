# Source adapter contract

**Phiên bản:** 1.0.0  
**Mục tiêu:** Chuẩn hóa việc tích hợp dataset và provider mà không làm rò rỉ upstream semantics vào domain

## 1. Source categories

- Static/released dataset.
- Incremental API dataset.
- Community product database.
- Manufacturer/restaurant feed.
- Internal curated dataset.
- External analysis provider.

Dataset adapter và analysis provider adapter có contract khác nhau.

## 2. Dataset adapter lifecycle

```text
discover
→ acquire
→ verify
→ preserve raw
→ parse
→ normalize
→ validate
→ map/stage
→ report
→ approve
→ activate
→ monitor/rollback
```

## 3. Core interfaces

Pseudo-Rust:

```rust
trait DatasetAdapter {
    fn descriptor(&self) -> DatasetDescriptor;
    async fn discover_releases(&self) -> Result<Vec<SourceRelease>, SourceError>;
    async fn acquire(&self, release: &SourceRelease) -> Result<Artifact, SourceError>;
    fn verify(&self, artifact: &Artifact) -> Result<VerifiedArtifact, SourceError>;
    fn parse(&self, artifact: &VerifiedArtifact) -> Result<RecordStream, SourceError>;
    fn normalize(&self, record: RawRecord) -> Result<NormalizedSourceRecord, MappingError>;
}
```

Application service, không adapter, chịu trách nhiệm persistence, jobs, activation và audit.

## 4. Dataset descriptor

```text
source_code
source_name
owner/publisher
license/terms URL
attribution requirement
redistribution/cache constraints
supported release modes
expected formats
data categories
timezone/locale
contact/incident URL
```

## 5. Release metadata

- Upstream version/release date.
- Discovery timestamp.
- Artifact URI.
- Byte size.
- Checksum/signature.
- Content type/compression.
- Schema fingerprint.
- License version.
- Supersedes relation.

## 6. Raw preservation

Raw artifact và record payload phải immutable. Normalized data không thay thế raw. Object storage path content-addressed hoặc release-addressed; database lưu checksum và URI.

## 7. Schema drift

Adapter phải phát hiện:

- missing required fields;
- new/removed columns;
- type/enum changes;
- unit changes;
- identifier format changes;
- record-count anomaly.

Drift report block activation nếu severity vượt policy.

## 8. Idempotency

Cùng source/release/checksum:

- không tạo duplicate release;
- parse có thể retry;
- normalized records upsert theo source identity trong staging;
- activation không chạy hai lần;
- job state recoverable sau crash.

## 9. Validation report

Tối thiểu:

- record counts;
- parse success/failure;
- nutrient/unit coverage;
- invalid values;
- duplicate IDs;
- changed mappings;
- added/deprecated records;
- quality distribution;
- license/terms change;
- impact estimate.

## 10. Mapping contract

Normalized source record giữ:

- source identity;
- original description/names;
- food state/type;
- portion data;
- composition data;
- derivation/data type;
- upstream timestamps;
- raw record reference.

Mapping sang canonical food là separate reviewed relation.

## 11. Activation

Activation không copy/overwrite tùy tiện. Dùng release pointer hoặc validity window để selection policy biết active release. Activation transaction ghi actor, reason, impact report và previous release để rollback.

## 12. Rollback

Rollback phải:

- đổi active pointer;
- không xóa raw/new release;
- ghi audit event;
- không sửa analysis snapshots đã tạo;
- trigger cache/materialization invalidation nếu có.

## 13. External analysis provider contract

```rust
trait ExternalAnalysisProvider {
    async fn analyze(&self, request: ProviderRequest)
        -> Result<ProviderResult, ProviderError>;
    fn capabilities(&self) -> ProviderCapabilities;
    fn terms_snapshot(&self) -> TermsSnapshot;
}
```

ProviderResult phải lưu provider/version/time/warnings/raw reference và terms restrictions. Không map thẳng provider IDs thành canonical IDs.

## 14. Provider operational controls

- Timeout/circuit breaker.
- Rate limit/quota.
- Cost attribution.
- Retry only for safe/transient errors.
- Payload minimization.
- No provider call inside DB transaction.
- Raw response storage chỉ khi terms/privacy cho phép.

## 15. Test contract

Mỗi adapter cần:

- golden source fixtures;
- corrupt artifact tests;
- schema drift tests;
- idempotency tests;
- pagination/retry tests;
- unit normalization tests;
- release activation/rollback integration tests;
- terms metadata test.
