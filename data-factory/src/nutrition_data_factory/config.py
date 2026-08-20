from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class FactoryConfig:
    config_path: Path
    artifact_root: Path
    source_registry_path: Path

    @classmethod
    def load(cls, path: Path) -> "FactoryConfig":
        config_path = path.resolve()
        raw = _read_object(config_path)
        artifact_store = _object(raw, "artifact_store")
        source_metadata = _object(raw, "source_metadata")
        artifact_root = _relative_or_absolute(config_path.parent, _string(artifact_store, "root"))
        registry_path = _relative_or_absolute(
            config_path.parent,
            _string(source_metadata, "registry_path"),
        )
        return cls(
            config_path=config_path,
            artifact_root=artifact_root,
            source_registry_path=registry_path,
        )


def _read_object(path: Path) -> dict[str, Any]:
    with path.open("r", encoding="utf-8") as handle:
        value = json.load(handle)
    if not isinstance(value, dict):
        raise ValueError(f"{path} must contain a JSON object")
    return value


def _object(value: dict[str, Any], key: str) -> dict[str, Any]:
    item = value.get(key)
    if not isinstance(item, dict):
        raise ValueError(f"{key} must be a JSON object")
    return item


def _string(value: dict[str, Any], key: str) -> str:
    item = value.get(key)
    if not isinstance(item, str) or not item.strip():
        raise ValueError(f"{key} must be a non-empty string")
    return item


def _relative_or_absolute(base: Path, value: str) -> Path:
    candidate = Path(value)
    return candidate.resolve() if candidate.is_absolute() else (base / candidate).resolve()

