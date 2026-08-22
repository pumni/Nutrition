"""One-shot maintainer helper for regenerating the committed synthetic golden fixture.

CI never runs this script. It exists to make the fixture reproducible from the same producer code.
"""

from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))
sys.path.insert(0, str(ROOT / "tests"))

from test_catalog_handoff_v1 import compile_fixture  # noqa: E402


if __name__ == "__main__":
    target = ROOT.parent / "contracts" / "catalog-handoff" / "v1" / "fixtures" / "minimal-valid"
    compile_fixture(target)
    print(target)
