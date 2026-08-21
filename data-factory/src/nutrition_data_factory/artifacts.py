from __future__ import annotations

import hashlib
import os
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any


class ArtifactConflictError(RuntimeError):
    """Raised when content-addressed storage contains bytes for the same digest that differ."""


class ArtifactIntegrityError(ValueError):
    """Raised when caller-supplied acquisition integrity metadata does not match bytes."""


@dataclass(frozen=True)
class AcquisitionMetadata:
    source_code: str
    publisher: str
    release: str
    retrieved_at: str
    filename: str
    size: int
    content_type: str
    sha256: str
    rights_state: str
    acquisition_tool_version: str

    def to_dict(self) -> dict[str, Any]:
        return {
            "source_code": self.source_code,
            "publisher": self.publisher,
            "release": self.release,
            "retrieved_at": self.retrieved_at,
            "filename": self.filename,
            "size": self.size,
            "content_type": self.content_type,
            "sha256": self.sha256,
            "rights_state": self.rights_state,
            "acquisition_tool_version": self.acquisition_tool_version,
        }


@dataclass(frozen=True)
class ArtifactRef:
    sha256: str
    size: int
    content_type: str
    relative_path: str
    acquisition: AcquisitionMetadata | None = None

    def to_dict(self) -> dict[str, object]:
        value: dict[str, Any] = {
            "sha256": self.sha256,
            "size": self.size,
            "content_type": self.content_type,
            "relative_path": self.relative_path,
        }
        if self.acquisition is not None:
            value["acquisition"] = self.acquisition.to_dict()
        return value


class ArtifactStore:
    """Small immutable, content-addressed artifact store."""

    def __init__(self, root: Path) -> None:
        self.root = root.resolve()
        self.root.mkdir(parents=True, exist_ok=True)

    def put_bytes(
        self,
        payload: bytes,
        content_type: str = "application/octet-stream",
        *,
        expected_sha256: str | None = None,
        expected_size: int | None = None,
        acquisition: AcquisitionMetadata | None = None,
    ) -> ArtifactRef:
        digest = hashlib.sha256(payload).hexdigest()
        if expected_sha256 is not None and digest != expected_sha256.lower():
            raise ArtifactIntegrityError(
                f"SHA-256 mismatch: expected {expected_sha256.lower()}, actual {digest}"
            )
        if expected_size is not None and len(payload) != expected_size:
            raise ArtifactIntegrityError(
                f"size mismatch: expected {expected_size}, actual {len(payload)}"
            )
        if acquisition is not None:
            if acquisition.sha256 != digest or acquisition.size != len(payload):
                raise ArtifactIntegrityError("acquisition metadata does not match artifact bytes")
        target_dir = self.root / digest[:2]
        target = target_dir / digest[2:]
        target_dir.mkdir(parents=True, exist_ok=True)
        if target.exists():
            existing = target.read_bytes()
            if existing != payload:
                raise ArtifactConflictError(
                    f"artifact collision at {target} for SHA-256 {digest}"
                )
        else:
            self._atomic_create(target, payload)
        return ArtifactRef(
            sha256=digest,
            size=len(payload),
            content_type=content_type,
            relative_path=target.relative_to(self.root).as_posix(),
            acquisition=acquisition,
        )

    def read(self, reference: ArtifactRef) -> bytes:
        path = self.root / Path(reference.relative_path)
        payload = path.read_bytes()
        actual = hashlib.sha256(payload).hexdigest()
        if actual != reference.sha256:
            raise ArtifactConflictError(
                f"artifact verification failed for {reference.relative_path}: {actual}"
            )
        return payload

    @staticmethod
    def _atomic_create(target: Path, payload: bytes) -> None:
        descriptor, temporary_name = tempfile.mkstemp(
            prefix=f".{target.name}.",
            dir=target.parent,
        )
        try:
            with os.fdopen(descriptor, "wb") as handle:
                handle.write(payload)
                handle.flush()
                os.fsync(handle.fileno())
            try:
                os.link(temporary_name, target)
            except FileExistsError:
                existing = target.read_bytes()
                if existing != payload:
                    raise ArtifactConflictError(f"artifact changed during create: {target}")
        finally:
            try:
                os.unlink(temporary_name)
            except FileNotFoundError:
                pass
