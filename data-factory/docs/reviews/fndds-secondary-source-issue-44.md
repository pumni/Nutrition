# FNDDS bounded secondary-source review — issue #44

Status: deterministic source review complete; one proposal is pending human/domain review. No
Vietnamese mapping is approved, no staged candidate package was created, and production activation
was not attempted.

The review was generated from the requested main baseline `c72361c60efa9a0467b7898d26d5e5748410a198`.
The report JSON is [`fndds-secondary-source-issue-44.json`](fndds-secondary-source-issue-44.json),
SHA-256 `8cb33094aa51a6b02659c314298d3e106530a7d0306ab0c162f5e865fee5dc50`.

## Source pin and rights

| Evidence | Value |
|---|---|
| USDA release | FNDDS 2021–2023, published October 2024 |
| Official archive | [`FoodData_Central_survey_food_json_2024-10-31.zip`](https://fdc.nal.usda.gov/fdc-datasets/FoodData_Central_survey_food_json_2024-10-31.zip) |
| Archive SHA-256 | `dfb06ae7ddc397ccd570b91c14b75438ab2ba39f64f22d321f61d4a52a77f3eb` |
| Archive member | `surveyDownload.json` |
| Member SHA-256 | `2e7eb9fda92adf1d4d784dba5eaa3a7fd4418cd86ccff383c7c9294d79e9b808` |
| JSON schema fingerprint | `bc072c4e5cf81ebb632ddb099d502e4e8a5cfef55933b9a18a60840a86e6e4f4` |
| Project decision | [`#41 owner decision`](https://github.com/pumni/Nutrition/issues/41#issuecomment-5867171847) |
| Foundation-primary state | [`#35 accepted checkpoint`](https://github.com/pumni/Nutrition/issues/35#issuecomment-5863469233); no Foundation source mapping is accepted for these targets |
| Rights/use evidence | [USDA FDC API Guide](https://fdc.nal.usda.gov/api-guide/) states FoodData Central data are public domain and CC0 1.0; it requests source attribution |

The Data Factory source registry was not promoted. Its SHA-256 is
`9a05078804e98f9ff7e17d7a6815a035dc140a759c5fb6f385037547b50716da`; `usda_fndds` remains
`reference_only`, allows `analysis` and `reference`, prohibits `staged_candidate`, and has
`production_eligible: false`. The new runner checks that state before it can write a report.

## Import and nutrient validation

Importer `fndds-survey-json-0.1.0` parsed all 5,432 source records: 5,432 accepted and 0 rejected.
The source-specific required-nutrient mapping is `fndds-required-nutrients-0.1.0`:

| FNDDS code | FDC nutrient ID | Internal completeness code | Unit |
|---:|---:|---|---|
| 208 | 1008 or 2047 | `energy_kcal` | kcal |
| 203 | 1003 | `protein_g` | g |
| 205 | 1005 | `carbohydrate_g` | g |
| 204 | 1004 | `fat_g` | g |

This uses the FNDDS-specific crosswalk in [USDA FNDDS 2021–2023 documentation, Appendix K](https://www.ars.usda.gov/ARSUserFiles/80400530/pdf/fndds/2021_2023_FNDDS_Doc.pdf).
It does not reuse the Foundation `fdc_energy_v1` rule, which selects FDC nutrient ID 2048. The FNDDS
JSON reports values per 100 g edible portion. The review packet records completeness and source
nutrient IDs, codes, labels, and units, but no nutrient amounts. The pinned JSON has no per-nutrient
derivation method field; all 65 retained nutrient definitions record method availability as false.

Required nutrient completeness passed for 5,431 records. FNDDS code `11000000` (`Milk, human`) has
no nutrient profile, as described in the USDA documentation; it is recorded as an expected absence.
No other required profile was incomplete or ambiguous.

## Bounded target result

The candidate policy covers only the seven issue #44 targets. It selects candidates by exact FNDDS
food code, FDC ID, description, and category; it does not rank similar descriptions or use fuzzy
matching. Foundation is primary. A target supplied as having an accepted Foundation mapping is
suppressed from FNDDS candidate generation.

One review packet was produced:

| Target | FNDDS food code | FDC ID | Exact description | Record SHA-256 | Nutrients |
|---|---:|---:|---|---|---|
| `cơm trắng` | `56205008` | `2708408` | `Rice, white, cooked, no added fat` | `634cf6fabbcc9ccb76f513ef274a9dcc3d92f495c78792cf8221170d98a102cc` | Required profile complete |

The exact description satisfies the current #35 source constraints for white, cooked rice. The
record does not specify rice cultivar. Its packet is `pending_human_review`, has no reviewer
reference, and has `reviewer_approved: false`. The packet includes the source/release/artifact/schema
hashes and the rejected alternatives: rice with unspecified fat state, rice made with oil, and
glutinous rice. This remains a proposal, not a Vietnamese food identity or preparation decision.

The other six targets have no proposal:

| Target | Result |
|---|---|
| `trứng gà luộc` | The source record says “boiled or poached”; the preparation is ambiguous. |
| `thịt bò` | `Beef, NFS` is not an exact identity/preparation match for the benchmark's stir-fried beef. |
| `thịt gà` | The source record leaves part and cooking method unspecified; no mapping was proposed from that profile. |
| `thịt gà luộc` | No source description contains both literal terms “chicken” and “boiled”; a stewed record was rejected. |
| `sữa tươi` | `Milk, NFS` uses a profile across milk fat classes, so it is not an exact fluid-milk identity. |
| `rau muống` | No exact “water spinach” or “morning glory” description exists; “Spinach, raw” was not substituted. |

USDA describes NFS/NS codes as used when survey detail is unavailable; their profiles use consumption
data and related population-level sources. The [FNDDS documentation, NFS/NS section](https://www.ars.usda.gov/ARSUserFiles/80400530/pdf/fndds/2021_2023_FNDDS_Doc.pdf)
also explains the source proportions for `Milk, NFS`. These records were kept out of the candidate
set rather than treated as exact foods.

## Evidence and runtime boundaries

The report records 7 targets, 1 candidate packet, 6 no-proposal outcomes, 0 parser rejections, and
`production_eligible: false`, `staged_candidate_created: false`, and `activation_attempted: false`.
FNDDS portion rows are included in the pinned source hash but no portion descriptions or weights are
copied into review packets. The output does not enter the catalog handoff, the Foundation release
compiler, backend ingestion, #36 portion evidence, or production.

Reproduce the report from `data-factory` with the pinned archive at
`artifacts/raw/usda_fndds/2021-2023/FoodData_Central_survey_food_json_2024-10-31.zip`:

```text
python scripts/run_fndds_secondary_review.py \
  --archive artifacts/raw/usda_fndds/2021-2023/FoodData_Central_survey_food_json_2024-10-31.zip \
  --output docs/reviews/fndds-secondary-source-issue-44.json
```

The run is offline after archive acquisition and refuses to replace a different report at the same
versioned path. #35, #36, #41, #33, source/release governance, contracts, migrations, and production
semantics were not changed.
