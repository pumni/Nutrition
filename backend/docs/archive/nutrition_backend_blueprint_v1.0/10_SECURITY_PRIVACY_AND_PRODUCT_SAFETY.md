# Security, privacy và product safety

**Phiên bản:** 1.0.0  
**Trạng thái:** Mandatory release gate  
**Scope:** Application, data, providers, product behavior


## 1. Data classification

| Data | Classification | Notes |
|---|---|---|
| Public food catalog | Public/internal curated | Có license obligations |
| Raw dataset artifacts | Public/licensed/restricted theo source | Không mặc định expose |
| Meal text/history | Sensitive personal data | Có thể tiết lộ sức khỏe/thói quen |
| Account identifiers | Personal data | Tách khỏi analytics khi có thể |
| LLM prompt/response | Sensitive transient | Có meal text |
| Corrections | Personal + product signal | Aggregate cẩn thận |
| Audit logs | Security-sensitive | Access restricted |

## 2. Threat model

### External attacker

- Credential/session theft.
- BOLA/IDOR đọc meal khác.
- Injection.
- Resource exhaustion.
- Enumeration.

### Malicious input

- Prompt injection.
- Oversized/complex text.
- Unicode abuse.
- Attempts to expose prompt/system details.

### Supply chain

- Compromised crate/container.
- Malicious dataset artifact.
- Dependency vulnerability.

### Insider/operator

- Excessive access to meal history.
- Raw prompt logging.
- Curator abuse/incorrect publish.

### Data integrity

- Import tampering.
- Source mapping corruption.
- Silent recipe/profile overwrite.

## 3. Authentication and sessions

- Use mature OIDC/OAuth identity provider hoặc secure first-party auth.
- Short-lived access tokens; refresh rotation nếu dùng.
- Secure, HttpOnly, SameSite cookies cho browser where appropriate.
- MFA bắt buộc cho curator/admin.
- Re-authentication cho high-impact admin actions.

## 4. Authorization

Mọi object read/write check ownership hoặc role. Không tin `user_id` từ request body.

Curation permissions:

- `catalog_viewer`.
- `curator`.
- `domain_reviewer`.
- `data_importer`.
- `admin`.

High-impact actions như merge food/publish factor policy cần reviewer hoặc four-eyes workflow.

## 5. Database security

- Separate DB roles: migration, API read/write, worker, curator.
- No superuser runtime.
- TLS connections.
- Secrets rotated.
- Parameterized SQL.
- Sensitive tables restricted.
- Audit privileged access.
- Backup encryption.

RLS có thể dùng defense-in-depth cho user data, nhưng phải test connection pooling/session context đúng.

## 6. Encryption

- TLS in transit.
- Managed encryption at rest.
- Application-level encryption cho raw meal text nếu threat model/requirements cần.
- Encryption key management tách database credentials.
- Search trên raw text không phải requirement mặc định; tránh đánh đổi encryption để search tùy tiện.

## 7. Data minimization

LLM request chỉ gồm:

- Meal text.
- Locale.
- Minimal parser schema context.

Không gửi:

- Email.
- Full user profile.
- Unrelated meal history.
- Internal auth IDs.

Personalization sau này dùng derived features tối thiểu và explicit policy.

## 8. Logging and telemetry

Prohibited by default:

- Raw meal text.
- Prompt/response bodies.
- Authorization headers.
- Database URLs.
- Source payload chứa personal data.

Dùng:

- Hash/fingerprint khi cần correlation.
- Item count.
- Error codes.
- Model/policy version.
- Quality labels.

Debug sampling production cần access control, expiry và approval.

## 9. LLM/provider security

- Provider contract/data retention review.
- Disable training/data retention where supported and required.
- Timeout/circuit breaker.
- No provider tool execution.
- Strict structured output.
- Output size limits.
- Treat model output untrusted.
- No secrets/catalog admin data in prompt.

## 10. Prompt injection controls

- Delimit untrusted text.
- Instruct only extraction.
- No tool access from parser model.
- Validate semantic spans.
- Do not expose hidden prompts.
- Red-team corpus.
- Reject/flag attempts without punishing normal text containing words like “ignore”.

## 11. API abuse controls

- Body length/item limits.
- Per-user/IP/device rate limits.
- Concurrency limits.
- Token/cost budgets.
- Idempotency.
- Bot/anomaly controls proportional to risk.
- `429` với retry semantics.

## 12. Dataset supply-chain security

- HTTPS/source allowlist.
- Checksum.
- Store immutable artifact.
- Parse in restricted worker/container.
- File size/decompression limits.
- Malware scanning where applicable.
- Importer version trace.
- No arbitrary code/macros from source files.

## 13. Privacy lifecycle

### Collection

Explain purpose and estimate nature.

### Use

Only analysis/history/improvement consistent with consent/policy.

### Retention

Suggested baseline pending legal/product review:

- Anonymous raw meal text: short TTL.
- Account meal history: until user deletion or retention setting.
- Provider payload: minimum possible.
- Aggregated de-identified quality metrics: longer if policy permits.

### Deletion

- Delete/anonymize account meal data.
- Delete raw text/ciphertext and user links.
- Preserve minimal legal/security audit if required, documented.
- Propagate to derived stores/backups according to policy.

## 14. Data export

User export includes their meal analyses/revisions in portable format. Không expose proprietary raw source records; include human-readable provenance.

## 15. Product safety

### Estimate framing

- Use “ước tính”.
- Show assumptions.
- Do not imply medical-grade accuracy.
- Encourage professional guidance only where context warrants, without alarmism.

### No diagnostic/prescriptive behavior

Backend returns nutrition information, not diagnosis, disease treatment or medication advice.

### Teen/user wellbeing safeguards

- Không tạo punitive score cho người ăn.
- Không cổ vũ hạn chế calories cực đoan.
- Không gắn đạo đức “tốt/xấu” lên món ăn.
- Không so sánh cơ thể người dùng với chuẩn hình thể.
- Nếu product thêm goals, cần separate safety review và age-aware design.

## 16. Incident response

Runbooks:

- Account/data exposure.
- LLM provider leak/misconfiguration.
- Malicious dataset release.
- Catalog corruption.
- Secret compromise.
- Incorrect nutrition release with broad impact.

Actions:

1. Contain/disable feature or release.
2. Preserve audit evidence.
3. Assess affected users/data/results.
4. Roll back catalog/policy.
5. Notify per legal/product obligations.
6. Postmortem and regression test.

## 17. Security release gates

- Dependency/container scan.
- Secret scan.
- Authorization tests.
- Migration least privilege.
- Prompt injection suite.
- Log redaction test.
- Backup encryption/restore.
- Provider config review.
- Data license review.

## 18. Compliance posture

Không tuyên bố HIPAA/GDPR hoặc chuẩn cụ thể chỉ dựa trên kiến trúc. Compliance phụ thuộc thị trường, legal role, contracts, processing purpose và operation. Blueprint hỗ trợ privacy-by-design nhưng cần legal assessment trước launch.

## 19. v1.0 external provider privacy controls

Before enabling a provider:

- review data retention/training terms;
- send only meal text/context required;
- strip user identity and unrelated history;
- document region/data transfer;
- configure opt-out/enterprise privacy controls where available;
- enforce deletion/cache restrictions;
- include provider in privacy inventory.

## 20. Product analytics minimization

Allowed events use opaque IDs/status/categories. Do not send raw meal text, ingredient free text, health notes or full provider responses to generic analytics platforms.

## 21. Correction and curation privacy

User corrections may contain personal context. Promotion pipeline must aggregate/de-identify before curator review unless explicit access is necessary and authorized.

## 22. Safety behavior under insufficient evidence

The system must prefer:

```text
clarify
or return insufficient evidence
```

over inventing an exact calorie value. It must not frame an uncertain result as a reason for restrictive eating, compensation or medical action.

## 23. Security release additions

- Source adapter supply-chain tests.
- Provider contract/terms snapshot.
- Stale clarification/revision authorization tests.
- Object-storage artifact integrity.
- Admin merge/publish privilege separation.
- Incident plan for corrupted data release, not only application compromise.
