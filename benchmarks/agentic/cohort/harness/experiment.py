"""Freeze, export and report a planned cohort without launching a model provider."""
import argparse
import json
from pathlib import Path
import sys

from experiment_plan import freeze, read_json, validate_plan, write_plan
from experiment_report import build_report, load_receipts, render, schedule_rows


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    freezing = commands.add_parser("freeze", help="validate design and exclusively create a frozen plan")
    freezing.add_argument("design")
    freezing.add_argument("plan")
    scheduling = commands.add_parser("schedule", help="emit planned trial metadata as JSONL; does not execute")
    scheduling.add_argument("plan")
    reporting = commands.add_parser("report", help="write plan-bound JSON and Markdown comparisons")
    reporting.add_argument("plan")
    reporting.add_argument("out", help="output prefix; both new files must be absent")
    reporting.add_argument("receipts", nargs="*", help="terminal receipt JSONL files; none means all trials missing")
    for command in (scheduling, reporting):
        command.add_argument("--partition", choices=("development", "holdout"), default="development")
    args = parser.parse_args(argv)
    try:
        if args.command == "freeze":
            plan = freeze(read_json(args.design))
            write_plan(args.plan, plan)
            print(plan["plan_sha256"])
            return 0
        plan = validate_plan(read_json(args.plan))
        if args.command == "schedule":
            for row in schedule_rows(plan, args.partition):
                print(json.dumps(row, sort_keys=True, allow_nan=False))
            return 0
        rows = load_receipts(plan, args.receipts)
        report = build_report(plan, rows, partition=args.partition)
        markdown = render(report)
        payload = json.dumps(report, indent=2, allow_nan=False) + "\n"
        destinations = (Path(args.out + ".json"), Path(args.out + ".md"))
        if any(path.exists() for path in destinations):
            raise ValueError("report output exists; choose a new prefix to retain prior evidence")
        # Reserve both destinations before publishing either representation. A
        # concurrent writer cannot have an existing result silently replaced.
        created = []
        try:
            with destinations[0].open("x", encoding="utf-8") as json_file:
                created.append(destinations[0])
                with destinations[1].open("x", encoding="utf-8") as markdown_file:
                    created.append(destinations[1])
                    json_file.write(payload)
                    markdown_file.write(markdown)
        except OSError:
            for path in created:
                path.unlink(missing_ok=True)
            raise
        print(markdown)
        # Zero means comparable instrument evidence, never that an arm won.
        return 0 if report["comparison_complete"] else 1
    except (OSError, ValueError, TypeError) as error:
        print(f"experiment: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
