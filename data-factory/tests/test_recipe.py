from __future__ import annotations

import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

from nutrition_data_factory.recipe import calculate_recipe_composition, validate_recipe_evidence  # noqa: E402


class RecipeTests(unittest.TestCase):
    def test_recipe_requires_resolved_ingredients_yield_and_human_publication_review(self) -> None:
        recipe = {
            "recipe_id": "recipe-pho-test",
            "status": "published",
            "components": [{"source_food_id": "fdc:1", "resolved_weight_g": 100}],
            "cooked_total_weight_g": 90,
        }
        report = validate_recipe_evidence(recipe, known_source_food_ids={"fdc:1"})
        self.assertEqual(report["status"], "invalid")
        self.assertIn("published_recipe_missing_human_reviewer", {item["reason_code"] for item in report["errors"]})

    def test_recipe_calculation_is_deterministic_and_in_review(self) -> None:
        recipe = {
            "recipe_id": "recipe-test",
            "status": "in_review",
            "components": [
                {"source_food_id": "fdc:1", "resolved_weight_g": 100},
                {"source_food_id": "fdc:2", "resolved_weight_g": 50},
            ],
            "cooked_total_weight_g": 120,
        }
        values = {
            "fdc:1": [{"target_code": "protein_g", "value": 20, "basis_amount_g": 100}],
            "fdc:2": [{"target_code": "protein_g", "value": 10, "basis_amount_g": 100}],
        }
        result = calculate_recipe_composition(recipe, ingredient_values=values)
        self.assertEqual(result["status"], "in_review")
        self.assertEqual(result["values"][0]["value"], 20.833333333333332)
        self.assertTrue(result["human_review_required"])


if __name__ == "__main__":
    unittest.main()
