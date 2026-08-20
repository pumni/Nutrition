from __future__ import annotations

import hashlib
from typing import Any

from ..models import NormalizedRecord


def build_candidate_queue(records: list[NormalizedRecord]) -> list[dict[str, Any]]:
    queue: list[dict[str, Any]] = []
    for record in sorted(records, key=lambda item: item.source_id):
        identity = f"{record.source_code}:{record.release}:{record.source_id}".encode("utf-8")
        candidate_id = f"candidate-{hashlib.sha256(identity).hexdigest()[:20]}"
        queue.append(
            {
                "candidate_id": candidate_id,
                "source_code": record.source_code,
                "source_release": record.release,
                "source_id": record.source_id,
                "observed_label": record.original_label,
                "normalized_label": record.normalized_label,
                "machine_confidence": {
                    "value": None,
                    "authoritative": False,
                },
                "review_status": "proposal",
                "reviewer": None,
            }
        )
    return queue

