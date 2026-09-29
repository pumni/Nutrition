# Avocado issue #35 review candidate, scope 0.2.0

This packet is generated from the versioned single-target scope, the pinned FNDDS archive, and the
current FNDDS source-registry safety policy. From `data-factory`, reproduce it with:

```powershell
python scripts/run_fndds_secondary_review.py `
  --archive artifacts/raw/usda_fndds/2021-2023/FoodData_Central_survey_food_json_2024-10-31.zip `
  --policy config/fndds-secondary-review-policy-0.2.0.json `
  --packet-target vmb-public-0004 `
  --output docs/reviews/vietnamese-avocado-issue-35-review-candidate-0.2.0.json
```

The command validates the archive and all 5,432 FNDDS records before writing the one packet. It refuses
to overwrite a different artifact at the versioned output path. The expected packet SHA-256 is
`2ef63070571a90bb11f9e795ffa23bc3157687885fcb46c77898be4103296050`.

This generated 0.2.0 packet replaces the earlier hand-assembled 0.1.0 packet from the draft PR.

The policy scope contains only `vmb-public-0004` (`1/2 quả bơ`). Its FNDDS candidate is a pending
semantic proposal. Foundation remains primary: the cited Hass-specific observation is not selected as
generic evidence. The packet does not approve a `bơ` alias or raw-preparation equivalence, and it carries
no portion or recipe evidence or production authorization. The historical seven-target #44 report and
its v0.1 policy remain unchanged.
