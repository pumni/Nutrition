from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.release.gate import build_release_gate_report  # noqa: E402


class ReleaseGateTests(unittest.TestCase):
    def test_review_packet_never_converts_review_requirements_into_pass(self) -> None:
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            package = root / "package"
            run = root / "run"
            package.mkdir()
            run.mkdir()
            self._write(package / "manifest.json", {
                "production_eligible": False,
                "impact_report_sha256": "placeholder",
                "source_releases": [{"artifact_sha256": "a" * 64}],
            })
            self._write(package / "source-releases.json", {"sources": [{"rights_state": "approved"}]})
            self._write(package / "validation-report.json", {"passed": True})
            impact = {"changes": {}, "fixed_meal_impact": []}
            self._write(package / "impact-report.json", impact)
            manifest = json.loads((package / "manifest.json").read_text(encoding="utf-8"))
            manifest["impact_report_sha256"] = hashlib.sha256((package / "impact-report.json").read_bytes()).hexdigest()
            self._write(package / "manifest.json", manifest)
            self._write(package / "normalized-records.jsonl", {"source_id": "1", "attributes": {"canonical_nutrients": [{"target_code": "protein_g"}]}})
            self._write(package / "curation-queue.jsonl", {"source_id": "1"})
            checksums = []
            for path in sorted(package.iterdir()):
                if path.name == "checksums.sha256" or not path.is_file():
                    continue
                checksums.append(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.name}")
            (package / "checksums.sha256").write_text("\n".join(checksums) + "\n", encoding="utf-8")
            self._write(run / "source-quality-report.json", {"passed": False, "errors": [{"reason_code": "negative_numeric_value"}]})
            self._write(run / "quality-report.json", {"passed": True, "errors": []})
            self._write(run / "compatibility-report.json", {"status": "matched", "observed_selection_sha256": "b" * 64})
            self._write(run / "acquisition-report.json", {"extracted_artifact": {"sha256": "a" * 64}})
            compatibility = root / "compatibility.json"
            self._write(compatibility, {"selection_sha256": "b" * 64, "reviewer": "pumni", "approval_reference": "ref"})

            report = build_release_gate_report(package, run, compatibility)

            self.assertEqual(report["status"], "review_required")
            self.assertFalse(report["production_eligible"])
            self.assertTrue(any(item["gate_id"] == "quality" and item["status"] == "review_required" for item in report["checks"]))
            self.assertTrue(any(item["gate_id"] == "curation" or item["gate_id"] == "identity_curation" for item in report["checks"]))

    @staticmethod
    def _write(path: Path, value: object) -> None:
        if path.suffix == ".jsonl":
            payload = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")) + "\n"
        else:
            payload = json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
        path.write_text(payload, encoding="utf-8")


if __name__ == "__main__":
    unittest.main()
