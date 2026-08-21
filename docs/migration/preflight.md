# Monorepo migration preflight

Migration timestamp: 2026-08-21T22:00:17.6910793+07:00

## Source repositories

| Repository | Visibility | Branch | Frozen source HEAD | Tags | Workflows | Submodules | Git LFS |
| --- | --- | --- | --- | --- | --- | --- | --- |
| pumni/Nutrition_backend | public | main | 555a0fcc932b534b262c35a23b6da7e5192e648f | none | .github/workflows/ci.yml | no | no tracked files |
| pumni/Nutrition_data_factory | public | main | 577c947fa5736716fcd8612e639f0768f44c66ac | none | .github/workflows/test.yml | no | no tracked files |

Backend repository metadata also included .github/dependabot.yml and SECURITY.md. Neither source repository contains a CODEOWNERS file.

Both remote main refs matched the frozen heads before migration. The target repository pumni/Nutrition was not present at preflight time.

## Baseline verification

The exact frozen snapshots passed before history rewrite:

Backend:

- cargo xtask check
- cargo xtask postgres
- cargo xtask fdc
- cargo xtask containers

Data Factory:

- python scripts/test.py — 31 tests passed
- python -m compileall -q src tests scripts

## Migration safety

The source working checkout was not used as the rewrite input. Mirror clones were rewritten in an isolated temporary workspace. Historical release evidence and legacy repository references remain unchanged inside the imported component histories; current root metadata is the only new repository-level reference.
