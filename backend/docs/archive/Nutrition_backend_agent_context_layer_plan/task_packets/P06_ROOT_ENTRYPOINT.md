# P06 — Root `AGENTS.md`

## Allowed path

- `AGENTS.md`

## Objective

Activate a small vendor-neutral bootloader only after the ACL validates.

## Required content

- implementation executor role;
- architect decision authority;
- task packet required before writes;
- `.agent/manifest.json` path;
- context profile required;
- profile is not selected by executor;
- exact block behavior;
- only allowed paths may change;
- verification/report mandatory;
- pointer to authority contract.

## Prohibited content

- copied blueprint;
- long coding style guide;
- vendor commands;
- model-specific prompting;
- task-specific decisions.

## Budget

<= 4096 bytes.

## Acceptance

Default ACL verifier passes after file creation.
