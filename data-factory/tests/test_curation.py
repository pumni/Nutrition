from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "scripts"))

from nutrition_data_factory.adapters.fdc_foundation import FdcSourceRecord  # noqa: E402
from nutrition_data_factory.curation.extractor import (  # noqa: E402
    ExtractionPolicy,
    extract_candidates,
)
from nutrition_data_factory.curation.packets import (  # noqa: E402
    CurationDecision,
    DecisionHistory,
    build_review_packet,
    decision_id,
)


class CurationTests(unittest.TestCase):
    def test_candidate_extraction_is_deterministic_and_omits_private_context(self) -> None:
        payload = json.loads((ROOT / "tests" / "fixtures" / "vietnamese-corpus.synthetic.json").read_text(encoding="utf-8"))
        result = extract_candidates(payload, ExtractionPolicy())
        self.assertEqual(result.report["unique_candidate_count"], 2)
        self.assertFalse(result.report["nutrition_values_emitted"])
        top = result.candidates[0]
        self.assertEqual(top["phrase"], "cơm trắng")
        self.assertEqual(top["frequency"], 2)
        self.assertEqual(top["no_diacritic_search"], "com trang")
        self.assertIn("1 bát", top["quantity_units"])
        self.assertTrue(all("safe_context" not in ref for ref in top["context_refs"]))
        self.assertTrue(all("context_sha256" in ref for ref in top["context_refs"]))
        self.assertEqual(extract_candidates(payload, ExtractionPolicy()).candidates, result.candidates)

    def test_review_packet_is_proposal_and_machine_confidence_is_non_authoritative(self) -> None:
        candidate = {
            "candidate_id": "candidate-1",
            "phrase": "cơm trắng",
            "normalized_phrase": "cơm trắng",
            "preparation_tokens": [],
            "machine_confidence": {"value": 0.99, "authoritative": True},
        }
        packet = build_review_packet(
            candidate,
            candidate_concept={"concept_id": "concept-1"},
            source_foods=[{"source": "synthetic-source", "source_id": "x"}],
            evidence_refs=["fixture://candidate/1"],
        )
        self.assertEqual(packet["review_status"], "pending_human_review")
        self.assertFalse(packet["machine_confidence"]["authoritative"])
        self.assertIsNone(packet["decision"])

    def test_evidence_source_class_and_negation_are_preserved_for_review(self) -> None:
        result = extract_candidates(
            {
                "observations": [
                    {"case_id": "case-1", "phrase": "da gà", "context": "không ăn da gà", "source_class": "pending_human_review", "negated": True},
                    {"case_id": "case-1", "phrase": "thịt gà", "context": "chỉ ăn thịt", "source_class": "pending_human_review", "negated": False},
                ]
            },
            ExtractionPolicy(),
        )
        by_phrase = {item["phrase"]: item for item in result.candidates}
        self.assertEqual(by_phrase["da gà"]["source_classes"], ["pending_human_review"])
        self.assertEqual(by_phrase["da gà"]["negated_observation_count"], 1)

    def test_vietnamese_mapping_proposals_require_semantic_constraints(self) -> None:
        from run_fdc_full_release import _propose_vietnamese_source_foods

        def source_record(fdc_id: int, description: str) -> FdcSourceRecord:
            return FdcSourceRecord(
                fdc_id=fdc_id,
                description=description,
                data_type="Foundation",
                characteristics={},
                nutrients=(),
                portions=(),
                payload={},
                payload_sha256="0" * 64,
            )

        records = (
            source_record(1, "Rice, white, long grain, unenriched, raw"),
            source_record(2, "Rice, white, long grain, cooked"),
            source_record(3, "Egg, whole, raw, frozen, pasteurized"),
            source_record(4, "Egg, whole, hard-boiled"),
            source_record(5, "Beef, loin, tenderloin, cooked"),
            source_record(6, "Beef, generic, cooked"),
        )
        rice_proposals, rice_decision = _propose_vietnamese_source_foods("cơm trắng", records, "fdc")
        egg_proposals, egg_decision = _propose_vietnamese_source_foods("trứng gà luộc", records, "fdc")
        beef_proposals, _ = _propose_vietnamese_source_foods("thịt bò", records, "fdc")
        recipe_proposals, recipe_decision = _propose_vietnamese_source_foods("bún bò Huế", records, "fdc")

        self.assertEqual([item["source_id"] for item in rice_proposals], ["2"])
        self.assertEqual([item["source_id"] for item in egg_proposals], ["4"])
        self.assertEqual([item["source_id"] for item in beef_proposals], ["6"])
        self.assertEqual(rice_decision["mapping_decision"], "deferred")
        self.assertEqual(egg_decision["mapping_decision"], "deferred")
        self.assertEqual(recipe_proposals, [])
        self.assertEqual(recipe_decision["mapping_decision"], "recipe_required")

    def test_decision_history_is_human_only_and_non_destructive(self) -> None:
        history = DecisionHistory()
        first = CurationDecision(
            decision_id=decision_id("candidate-1", "2026-08-20T00:00:00Z", "reviewer@example"),
            decision_type="food_name",
            candidate_id="candidate-1",
            decision="deferred",
            reviewer="reviewer@example",
            reviewed_at="2026-08-20T00:00:00Z",
            rationale="Needs identity evidence.",
            evidence_refs=("fixture://candidate/1",),
            policy_version="curation-decision-0.1.0",
        )
        history.append(first)
        second = CurationDecision(
            decision_id=decision_id("candidate-1", "2026-08-21T00:00:00Z", "reviewer@example"),
            decision_type="food_name",
            candidate_id="candidate-1",
            decision="approved",
            reviewer="reviewer@example",
            reviewed_at="2026-08-21T00:00:00Z",
            rationale="Reviewed against the cited evidence.",
            evidence_refs=("fixture://candidate/1",),
            policy_version="curation-decision-0.1.0",
            supersedes=first.decision_id,
        )
        history.append(second)
        self.assertEqual(len(history.all()), 2)
        self.assertIn('"supersedes":"' + first.decision_id + '"', history.to_jsonl())
        with self.assertRaises(ValueError):
            history.append(
                CurationDecision(
                    decision_id="CUR-agent",
                    decision_type="food_name",
                    candidate_id="candidate-2",
                    decision="approved",
                    reviewer="AI-agent",
                    reviewed_at="2026-08-21T00:00:00Z",
                    rationale="not allowed",
                    evidence_refs=("fixture://candidate/2",),
                    policy_version="curation-decision-0.1.0",
                )
            )


if __name__ == "__main__":
    unittest.main()
