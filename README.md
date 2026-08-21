# Nutrition

Evidence-first nutrition monorepo.

```text
Nutrition/
├── backend/       Rust backend and PostgreSQL application
├── data-factory/  Offline deterministic evidence producer
└── contracts/     Shared versioned machine contracts (reserved)
```

The Data Factory produces staged evidence and candidate artifacts. The backend consumes validated
staged evidence. Production activation remains explicit, versioned, and human-controlled.

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
