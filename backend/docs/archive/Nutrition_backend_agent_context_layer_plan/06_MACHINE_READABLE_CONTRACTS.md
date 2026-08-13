# 06 — Machine-Readable Contracts

## 1. `manifest.json`

Required fields:

```json
{
  "schema_version": "1.0.0",
  "context_release": "agent-context-1.0.0",
  "project": {
    "repository": "pumni/Nutrition_backend",
    "behavior_release": "foundation-0.6.0"
  },
  "authority": {
    "architect_decides": true,
    "executor_implementation_only": true,
    "task_packet_required": true,
    "context_profile_required": true
  },
  "budgets": {
    "agents_md_max_bytes": 4096,
    "authority_file_max_bytes": 12288,
    "invariant_file_max_bytes": 12288,
    "context_file_max_bytes": 16384
  },
  "paths": {
    "profile_index": ".agent/profiles/context-profiles.json",
    "source_register": ".agent/maps/source-register.json",
    "source_lock": ".agent/state/source-lock.json",
    "verification_map": ".agent/maps/verification-map.json"
  }
}
```

Additional fields are allowed only if explicitly documented in `.agent/README.md`.

## 2. Task packet schema

Required semantic rules:

- `schema_version` = `1.0.0`
- `task_id` non-empty
- `objective` non-empty
- `context_profile` exists in profile index
- `decision_authority` = `architect`
- `executor_role` = `implementation_only`
- `decision_points` exists and length = 0
- `allowed_paths` non-empty
- `forbidden_paths` exists
- `implementation_sequence` non-empty and ordered
- `acceptance_criteria` non-empty
- `verification` non-empty
- impacts explicitly declared:
  - `runtime_behavior`
  - `domain_behavior`
  - `api`
  - `database`
  - `dependencies`
  - `behavior_versions`
- `escalation_conditions` non-empty
- `completion_report_required` = true

Impact values are:
- `"none"`
- `"specified_change"`

If an impact is `specified_change`, the packet must include an explicit `impact_spec`.

## 3. Verification report schema

Must record:

```text
task_id
baseline_commit
changed_files[]
checks[]:
  command
  exit_code
  status
  evidence
scope:
  all_changes_allowed
  forbidden_path_changes[]
impacts:
  runtime_behavior
  domain_behavior
  api
  database
  dependencies
  behavior_versions
result: pass|fail|blocked
```

## 4. Implementation report schema

Machine-readable JSON schema exists, while the human-readable report template is Markdown.

Required content:
- scope summary;
- changed files;
- acceptance criteria mapping;
- verification evidence;
- impact declaration;
- deviations;
- blockers.

## 5. Source register

`maps/source-register.json` maps each derived ACL artifact to one or more canonical repository sources.

Example:

```json
{
  ".agent/invariants/llm-boundary.md": [
    "docs/HOSTED_PARSER.md",
    "docs/FOUNDATION_DECISIONS.md",
    "nutrition_backend_blueprint_v1.0/12_ARCHITECTURE_DECISION_RECORDS.md"
  ]
}
```

## 6. Source lock

`state/source-lock.json` contains:

```json
{
  "schema_version": "1.0.0",
  "algorithm": "SHA256",
  "sources": [
    {
      "path": "docs/FOUNDATION_DECISIONS.md",
      "sha256": "<computed>"
    }
  ]
}
```

The executor computes hashes mechanically. It does not decide which sources belong in the lock; the list is specified by the implementation packet.

## 7. Change impact map

Each path rule contains:
- glob/pattern;
- recommended profile (for architect use);
- mandatory gates;
- risk tags;
- forbidden impacts unless task explicitly overrides.

The verifier uses path rules to check consistency. The executor does not use the map to autonomously broaden scope.
