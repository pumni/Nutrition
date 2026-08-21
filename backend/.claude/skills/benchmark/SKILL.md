---
name: benchmark
description: Work with VietnameseMealBench tooling while keeping benchmark evidence separate from production eligibility.
---

# Benchmark

Read [docs/evidence/vietnamese-meal-bench.md](../../../docs/evidence/vietnamese-meal-bench.md) and inspect the
fixture manifest before changing benchmark tooling. Keep parser gold, downstream analysis gold,
human adjudication, and sealed/challenge evidence distinct. Run `cargo xtask benchmark` and the
relevant local regression script. Do not fabricate annotations or claim production authorization.
