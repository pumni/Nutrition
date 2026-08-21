---
name: provider-change
description: Change the hosted parser or provider mapping without weakening validation, privacy, or fail-closed behavior.
---

# Provider change

Read [docs/architecture/parser.md](../../../docs/architecture/parser.md) and inspect direct parser tests.
Keep provider output untrusted, preserve schema and semantic validation, circuit behavior, and
privacy-safe telemetry. Exercise controlled/mock tests and the relevant benchmark checks. Never
use provider output to invent nutrition evidence or silently fall back to fixture parsing.
