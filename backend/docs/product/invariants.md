# Product invariants

The product is evidence-first. A bounded language parser may identify meal structure, but food
identity, portion mass, composition values, calories, and persisted evidence are resolved by
deterministic/versioned backend evidence.

- Unknown foods and unsupported portions fail closed.
- No universal hidden household-to-gram conversion is inferred.
- Published catalog evidence and completed analysis revisions are immutable and replayable.
- Snapshot hashes and behavior-version vectors remain part of persisted evidence.
- Production catalog activation, provider enablement, and release publication require explicit
  human-controlled gates.
- Development fixtures and benchmark tooling cannot authorize production traffic.

Detailed current behavior belongs in [foundation](../architecture/foundation.md), [portion
evidence](../evidence/portions.md), and [nutrition sources](../evidence/nutrition-sources.md).
