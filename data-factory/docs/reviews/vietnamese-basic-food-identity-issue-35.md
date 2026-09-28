# Vietnamese basic-food identity and source review packet

Status: `blocked_human_review_required`; proposal-only; no reviewer decision or staged release

This packet records the current issue #35 evidence boundary. It does not approve Vietnamese aliases,
source mappings, preparation equivalence, portions, recipes, or production activation.

## Verified baseline

The analysis used `main` at `557992f426921dd945fdddbb52e29b747cede1e6`. PR #39 was still open and had
no merge commit; its changes are not part of this baseline.

| Evidence | SHA-256 / value | Result |
|---|---|---|
| `config/source_registry.json` | `9a05078804e98f9ff7e17d7a6815a035dc140a759c5fb6f385037547b50716da` | FDC Foundation `2026-04-30` is an approved candidate source; unreviewed mappings remain prohibited. |
| USDA FDC archive | `186e988ec542e913f51ef62b86a47758e8cdd0d1dc3889e7b055581f3c09c77a` | Re-fetched from the pinned official USDA URI and hash-verified. |
| USDA extracted JSON | `27d1fe3fd89edfbe528ed915da5619320e1d004d4594603a1b19bdb1511590cc` | Hash-verified; the adapter parsed 363 records and quarantined 32 null rows. |
| VietnameseMealBench manifest | `b8af69b3c278158f1f3a20aeb65c52cb993c59a4d33bef2fa037431547913502` | Current hash. The previous value in `coverage.md` was stale after the manifest path update. |
| VietnameseMealBench public cases | `ad11b9061c4c150383590c43b10be15cda8cfb944f9ce2e295e49bd24cf96f32` | 15 public cases, 16 parsed items; annotations remain pending human review. |
| Test-only foundation seed | `457cbe5999e6559dadb70321a166f73f1955558a8b0d4c43f70839081e039e84` | Test fixture only; not production composition or portion evidence. |
| Existing M3 selection | `ad867dbbb6a9387c4cb3e3837fb337353097d7ebd99f774eded25cf56dd9ffc2` | Unchanged; this review did not add or select FDC IDs. |

The historical coverage snapshot records `pumni/Nutrition_backend@479ac773b372599e2648437bfe5b56620f1b706d`
as its backend regression baseline. That is snapshot metadata from the prior backend repository, not the
current monorepo `main` SHA above.

The committed Data Factory coverage snapshot (`data-coverage-evidence-0.1.0`, SHA-256
`eee235f61b6b219a0ff24a58b2da1412491ce81eeb76959cc5f936d945dd637c`) reports 15 Vietnamese identity
candidates, 15 review packets, and 0 semantically compatible source-mapping proposals. Its referenced
evidence-pack inputs and compiled package directory are not present in this checkout, so that historical
full package cannot be rebuilt byte-for-byte here. The pinned USDA source itself was available and was
checked independently against the current public benchmark phrases using mapping policy
`vietnamese-semantic-proposal-0.2.0`.

The Data Factory source-shape fingerprint (`2da322eea296a96ea304c68d235538b6d93412619e72a3cbe7c91032937139c1`)
and backend importer-contract fingerprint (`bfad93f4dac2af1d5fca89f3c69ddd34de34c945946c1bb15e625a138ebae5cf`)
describe different inputs: the former fingerprints observed JSON structure, while the latter hashes the
backend parser contract. Their values are not expected to match.

## Current identity and source-mapping result

The deterministic source analysis returned 0 source-mapping proposals for all 15 unique public benchmark
phrases. For the basic-food targets and broad `cơm` phrase, no record satisfied the configured semantic
constraints. Ambiguous, negated-only, and recipe-required phrases were excluded from source mapping by
policy. It emitted no source IDs or descriptions. Non-matching records were not ranked as nearest
alternatives. With no candidate source record, completeness for the required
`energy_kcal`, `protein_g`, `carbohydrate_g`, and `fat_g` values is **not assessable**; it is not recorded
as complete or incomplete.

The replayed code was `scripts/run_fdc_full_release.py` SHA-256
`3c3660e91bbdd132eec54b93210ab23e532185410b6f6469e03ba8e1d30afbf4`, adapter
`src/nutrition_data_factory/adapters/fdc_foundation.py` SHA-256
`067773543b8c0de5a6dafbdb52083e73deb9e52e601f49100a41c5194226c532`, and nutrient policy
`src/nutrition_data_factory/nutrients.py` SHA-256
`839f43c5dc722fa74c125b922769783d05669c408b1c622dbb6ee90e41390480`.

| Benchmark phrase / case | Canonical proposal, preparation, aliases | FDC source code/release; exact ID and description | Mapping constraint and alternatives | Composition completeness | Review state/reference |
|---|---|---|---|---|---|
| `cơm trắng` / `vmb-public-0002`, `vmb-public-0005` | Canonical candidate `cơm trắng`; preparation unspecified in annotations; no aliases proposed | `usda_fdc_foundation@2026-04-30`; no ID/description proposed | Requires rice, white, cooked; raw, dry, and flour records excluded. No nearest record was selected. | Not assessable; no source ID | Pending human review; reviewer/reference null |
| `trứng gà luộc` / `vmb-public-0005` | Canonical candidate `trứng gà luộc`; preparation `luộc`; no aliases proposed | `usda_fdc_foundation@2026-04-30`; no ID/description proposed | Requires egg, whole, boiled/hard-boiled; raw, frozen, and dried records excluded. No nearest record was selected. | Not assessable; no source ID | Pending human review; reviewer/reference null |
| `thịt bò` / `vmb-public-0003` | Canonical candidate `thịt bò`; preparation `xào`; no aliases proposed | `usda_fdc_foundation@2026-04-30`; no ID/description proposed | Requires generic beef; specific cuts and processed products are excluded. Preparation equivalence remains a separate review decision. | Not assessable; no source ID | Pending human review; reviewer/reference null |
| `thịt gà` / `vmb-public-0007` | Canonical candidate `thịt gà`; preparation unspecified; no aliases proposed | `usda_fdc_foundation@2026-04-30`; no ID/description proposed | Requires generic chicken; specific cuts and preparations are excluded. No nearest record was selected. | Not assessable; no source ID | Pending human review; reviewer/reference null |
| `thịt gà luộc` / `vmb-public-0012` | Canonical candidate `thịt gà luộc`; preparation `luộc`; no aliases proposed | `usda_fdc_foundation@2026-04-30`; no ID/description proposed | Requires chicken and boiled; specific cuts, raw/frozen/dried, and braised records are excluded. No nearest record was selected. | Not assessable; no source ID | Pending human review; reviewer/reference null |
| `sữa tươi` / `vmb-public-0010` | Canonical candidate `sữa tươi`; preparation unspecified; no aliases proposed | `usda_fdc_foundation@2026-04-30`; no ID/description proposed | Requires fluid milk without assuming a milk-fat class. No nearest record was selected. | Not assessable; no source ID | Pending human review; reviewer/reference null |
| `rau muống` / `vmb-public-0014` | Canonical candidate `rau muống`; preparation unspecified; no aliases proposed | `usda_fdc_foundation@2026-04-30`; no ID/description proposed | Requires water spinach/morning glory; unrelated leafy greens are excluded. No nearest record was selected. | Not assessable; no source ID | Pending human review; reviewer/reference null |

The following phrases remain unresolved or outside this basic-food mapping slice. No source ID,
description, nutrient completeness result, reviewer, or decision reference was emitted for these items:

- `bơ` (`vmb-public-0004`) may mean avocado or dairy butter; no identity or alias is selected.
- `trứng luộc` (`vmb-public-0001`, preparation `luộc`) does not specify egg type; it is not an alias for chicken egg.
- `cơm` (`vmb-public-0011`) is a broad alias candidate for `cơm trắng`; the token relationship is not an approval.
- `bánh mì` (`vmb-public-0013`) may mean bread or the composite sandwich dish; no identity is selected.
- `da gà` (`vmb-public-0007`) remains unsupported for v1 because the available observation is negated-only.
- `cơm gà` (`vmb-public-0006`), `phở bò` (`vmb-public-0008`, preparation `tái`), and `bún bò Huế` (`vmb-public-0009`) require recipe evidence and cannot become basic-food composition mappings.

The existing policy profiles include intent classifications for some phrases, but those are not curator
review records. No human reviewer, decision reference, or approval was supplied for this packet. Every
identity, alias, and source-mapping decision remains `pending_human_review`.

## Benchmark coverage boundary

The current deterministic backend coverage analysis reports 15 cases and 16 parsed items:

| Coverage dimension | Count | Interpretation |
|---|---:|---|
| Exact identity in the test-only seed | 3 | Fixture matches only; not real-source composition approval |
| Alias review needed | 2 | Candidate relationships only; aliases remain unapproved |
| Identity unsupported by the current seed | 8 | No exact identity in the fixture |
| Preparation mismatch | 0 | No matching case in this snapshot |
| Recipe evidence needed | 3 | Composite dishes remain outside this identity slice |
| Portion evidence needed | 12 | Includes items with unresolved identity or recipe evidence |

These dimensions are not a complete meal-analysis pass rate. No benchmark item is ready for real-data
calculation from this slice: the seed is test-only, no Vietnamese phrase has a compatible FDC mapping,
and the existing real portion and recipe evidence counts remain zero.

## Human decisions needed to unblock

1. For each basic-food identity above, provide a rights-cleared exact source record that satisfies the
   listed semantics, or leave that identity explicitly unsupported for this release. Do not relax the
   constraints to force a nearest match.
2. Decide whether `bơ`, `trứng luộc`, `cơm`, or `bánh mì` has enough context for a specific identity or
   must stay ambiguous. Record a named reviewer and decision reference before changing review state.
3. Keep physical portion measurements and recipe/yield evidence in their separate #36 and #37 review
   flows. This packet supplies no grams, portions, recipe ingredients, calories, or nutrient values.

No new staged candidate release was created because no mapping has been reviewed. The exact 20-record
M3 selection and all production activation state remain unchanged.
