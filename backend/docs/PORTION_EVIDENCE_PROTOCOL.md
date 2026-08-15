# Portion evidence measurement protocol

Status: proposed M1 domain protocol  
Protocol version: `portion-measurement-0.1.0`

## Purpose

Define how project-controlled household, count, and volume portion measurements become reviewable
nutrition evidence without converting language units into guessed gram weights.

A portion observation is not a universal unit conversion. It is evidence for a specific combination:

```text
food identity
× preparation state
× canonical measure and physical context
× quantity
→ measured mass distribution
```

This protocol does not add production portion values. It defines the evidence and release gates that
must be satisfied before values can be staged or activated.

## Evidence boundary

The hosted parser may extract a quantity and a unit phrase from user text. It must not supply,
infer, or repair a gram weight. Gram evidence comes only from a reviewed source or a measurement
study governed by this protocol.

Composition evidence and portion evidence remain independent. A food may have a valid composition
profile while a household measure for that food is still unknown.

## Measurement target

A target is identified by at least:

- internal food identity;
- preparation state;
- canonical measure class;
- physical measure context when relevant;
- represented quantity.

Examples of physical context include vessel capacity/dimensions, count-size class, drained versus
undrained state, packed versus loose fill, or other factors that materially change mass.

Evidence must not cross a target boundary merely because the same Vietnamese unit phrase appears in
both targets.

## Pilot target selection

The first study contains 10 reviewed measurement targets. They are selected from a representative
Vietnamese meal corpus or benchmark rather than from an arbitrary list of units.

Rank candidate `food × preparation × measure` tuples by:

1. observed/requested frequency;
2. expected contribution of portion uncertainty to nutrient uncertainty;
3. product importance;
4. reproducibility of the physical measure definition;
5. availability of an approved food identity and preparation definition.

The target list and ranking inputs are part of the study manifest. Changing the target list creates
a new study manifest; it does not silently modify a completed study.

## Independent samples and repeats

An **independent sample** is a separately prepared/acquired serving that contributes information
about real portion variability.

A repeated weighing of the same physical serving is a **measurement repeat**, not a new independent
sample. Repeats are useful for detecting instrument/handling error but must not inflate
`sample_count`.

The approved study plan declares the minimum independent sample count for each target before data
collection. A target that does not meet that plan remains incomplete and cannot be published.

Where practical, independent servings should span more than one acquisition or preparation batch so
the recorded bounds are not accidentally a single-batch property.

## Required measurement metadata

Each target record retains enough information to reproduce and audit the result:

- protocol version;
- study/release identifier;
- food ID and preparation-state definition;
- original unit phrase and canonical measure class;
- represented quantity;
- vessel capacity/dimensions or count-size class when applicable;
- acquisition/preparation batch identifiers;
- individual independent serving masses;
- any measurement repeats linked to their serving;
- instrument identifier and resolution;
- calibration/check information;
- tare method and tare mass when applicable;
- operator and measurement date;
- estimator/bound policy version;
- independent sample count;
- reviewer;
- notes about deviations or exclusions.

Raw study records should be retained as immutable evidence metadata/artifacts. The catalog
observation is a compiled view of that evidence, not the only copy of the measurements.

## Mass calculation policy

The measurement pipeline is deterministic. For each target it produces:

- a central `gram_weight`;
- `lower_gram_weight`;
- `upper_gram_weight`;
- independent `sample_count`.

For the pilot, the study manifest must declare the exact central estimator and bound method before
activation. Individual masses are retained so the result can be replayed under a future policy.

The following invariant is mandatory:

```text
0 < lower_gram_weight <= gram_weight <= upper_gram_weight
```

Do not narrow bounds merely to make nutrient estimates appear more precise. If real variability is
large, the backend propagates that uncertainty to the nutrition result.

Changing the estimator or bound method after release requires a new portion behavior version and an
impact report; existing active evidence is not edited in place.

## Existing database mapping

The current schema already supports the compiled observation needed by the runtime:

- `catalog.portion_observation` stores food-specific quantity/mass evidence, lower/upper bounds,
  source/review metadata, and sample count;
- `catalog.catalog_release_portion_observation` pins the observation to a catalog release;
- released evidence and memberships become immutable under the existing database guards.

Study-specific details that do not have dedicated columns belong in versioned evidence metadata or
a referenced source artifact. A schema migration is required only if a demonstrated replay/audit
requirement cannot be represented safely by the existing fields.

## Validation checklist

Before a measurement target can enter a staged catalog release, verify:

- [ ] food identity is reviewed and unambiguous;
- [ ] preparation state is explicit;
- [ ] measure context is reproducible;
- [ ] represented quantity is positive;
- [ ] instrument and tare metadata are complete;
- [ ] independent samples are distinguishable from repeats;
- [ ] independent sample count satisfies the approved study plan;
- [ ] every retained mass is positive;
- [ ] no unexplained sample was discarded;
- [ ] central estimate and bounds replay from retained measurements;
- [ ] lower <= central <= upper;
- [ ] source/study release and protocol version are recorded;
- [ ] assigned domain reviewer approved the compiled observation;
- [ ] validation/impact report and rollback target exist.

A failed item keeps the target out of production publication.

## Clarification and fail-closed behavior

Runtime behavior remains conservative:

- missing food/unit evidence → clarify or return insufficient evidence;
- ambiguous vessel/count size → clarify rather than select a hidden default;
- preparation mismatch → do not reuse the observation;
- parser/LLM-suggested gram weight without reviewed evidence → reject;
- another food's portion observation → do not extrapolate;
- evidence from a staged but inactive release → do not use for active analysis.

## Release workflow

```text
study manifest
→ measurement collection
→ deterministic compilation
→ validation report
→ domain review
→ staged catalog membership
→ explicit release activation
→ immutable active evidence
```

Activation is an independent human/release decision. This protocol must never make a worker import or
measurement script activate a catalog release automatically.

## First-study completion artifact

The first 10-target study should produce, as reviewable artifacts:

1. target manifest and selection rationale;
2. protocol/version identifier;
3. raw measurement records;
4. deterministic compilation report;
5. validation failures/exclusions, if any;
6. proposed staged observations;
7. impact report against representative meal cases;
8. reviewer decision and rollback target.

Only approved targets advance; incomplete targets may remain deferred without blocking unrelated
food/measure evidence.
