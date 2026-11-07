"""Print the canonical cohort report through the historical aggregation entrypoint."""
import argparse
from pathlib import Path
import sys

from scoreboard import build_report, load, render


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("receipts", nargs="*", help="JSONL receipt files from one or more cohorts")
    args = parser.parse_args(argv)
    paths = args.receipts or [Path(__file__).resolve().parent.parent / "receipts" / "cohort-2026-11-04.jsonl"]
    try:
        report = build_report(load(paths))
        print(render(report), end="")
    except (OSError, ValueError) as error:
        print(f"agg: {error}", file=sys.stderr)
        return 2
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
