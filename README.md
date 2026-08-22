# Nutrition

Evidence-first nutrition monorepo.

```text
Nutrition/
├── backend/       Rust backend and PostgreSQL application
├── data-factory/  Offline deterministic evidence producer
└── contracts/     Shared versioned machine contracts
```

The catalog handoff path is:

```text
Data Factory → contracts/catalog-handoff/v1 → Backend validator → PostgreSQL staged release → human activation gate
```

The Data Factory produces deterministic, checksum-bound evidence packages and never connects to
the backend database. The backend validates the exact package before a transaction and can create
only a staged release; production activation remains explicit, versioned, and human-controlled.

## Verify

Backend:

```powershell
cd backend
cargo xtask check
cargo xtask postgres
cargo xtask fdc
cargo xtask containers
```

Data Factory:

```powershell
cd data-factory
python scripts/test.py
python -m compileall -q src tests scripts
```

The components keep independent package and release lifecycles. The Rust workspace remains rooted
at `backend/Cargo.toml`; the Python project remains rooted at `data-factory/pyproject.toml`.
