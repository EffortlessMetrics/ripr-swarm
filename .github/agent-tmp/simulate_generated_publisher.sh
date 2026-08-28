#!/usr/bin/env bash
set -euo pipefail

rm -rf \
  target/generated-review-workflow \
  target/ripr \
  target/fake-bin \
  target/gh-requests.log \
  target/review-payload.json \
  target/existing-source.json \
  target/publish-step.sh \
  target/capture-step.sh
mkdir -p target/generated-review-workflow target/ripr/review target/fake-bin
cargo run -p ripr -- init --root target/generated-review-workflow --ci github

python - <<'PY'
from pathlib import Path

workflow = Path("target/generated-review-workflow/.github/workflows/ripr.yml").read_text()
expression_open = "$" + "{{"
replacements = {
    expression_open + " github.event.pull_request.head.sha }}": "deadbeef",
    expression_open + " github.repository }}": "EffortlessMetrics/ripr-swarm",
    expression_open + " github.event.pull_request.number }}": "123",
}

def extract(step_name: str, target: str) -> None:
    step = f"      - name: {step_name}\n"
    start = workflow.index("        run: |\n", workflow.index(step)) + len("        run: |\n")
    end = workflow.index("\n      - name:", start)
    lines = []
    for line in workflow[start:end].splitlines():
        if line:
            if not line.startswith("          "):
                raise SystemExit(f"unexpected {step_name} indentation: {line!r}")
            lines.append(line[10:])
        else:
            lines.append("")
    script = "#!/usr/bin/env bash\nset -euo pipefail\n" + "\n".join(lines) + "\n"
    for source, replacement in replacements.items():
        script = script.replace(source, replacement)
    Path(target).write_text(script)

extract("Publish RIPR inline comments", "target/publish-step.sh")
extract("Capture existing RIPR inline comments", "target/capture-step.sh")
PY
bash -n target/publish-step.sh
bash -n target/capture-step.sh

cat > target/ripr/review/comment-publish-plan.json <<'JSON'
{
  "summary": {
    "guidance_comments": 3,
    "summary_only": 3,
    "suppressed": 0,
    "publishable": 3,
    "blocked": 0,
    "safe_to_publish": true
  },
  "operations": [
    {
      "operation": "create",
      "safe_to_publish": true,
      "dedupe_key": "ripr:one",
      "placement": {"path": "src/lib.rs", "line": 10, "side": "RIGHT"},
      "body": "### ripr gap: missing boundary assertion\n\nWhy this matters:\nThis boundary is reachable.\n\nRepair:\nAdd one exact assertion for the threshold.\n\nVerify:\n`cargo test threshold`"
    },
    {
      "operation": "create",
      "safe_to_publish": true,
      "dedupe_key": "ripr:two",
      "placement": {"path": "src/lib.rs", "line": 20, "side": "RIGHT"},
      "body": "### ripr gap: weak oracle\n\nWhy this matters:\nThe current test only checks success.\n\nRepair:\nAssert the exact returned value.\n\nVerify:\n`cargo test exact_value`"
    },
    {
      "operation": "create",
      "safe_to_publish": true,
      "dedupe_key": "ripr:three",
      "placement": {"path": "src/lib.rs", "line": 30, "side": "RIGHT"},
      "body": "### ripr gap: missing output contract\n\nWhy this matters:\nOutput can drift.\n\nRepair:\nAdd one focused golden assertion.\n\nVerify:\n`cargo test golden`"
    }
  ]
}
JSON

cat > target/fake-bin/gh <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> target/gh-requests.log
if [[ "$*" == *"--method PATCH"* ]]; then
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--input" ]; then
      cp "$2" target/update-payload.json
      exit 0
    fi
    shift
  done
  echo "update request lacked --input" >&2
  exit 1
fi
if [[ "$*" == *"/reviews"* ]]; then
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--input" ]; then
      cp "$2" target/review-payload.json
      exit 0
    fi
    shift
  done
  echo "review request lacked --input" >&2
  exit 1
fi
if [[ "$*" == *"/comments"* ]] && [[ "$*" != *"--method PATCH"* ]]; then
  cat target/existing-source.json
  exit 0
fi
echo "unexpected gh request: $*" >&2
exit 1
SH
chmod +x target/fake-bin/gh target/publish-step.sh target/capture-step.sh

PATH="$PWD/target/fake-bin:$PATH" bash target/publish-step.sh
test "$(wc -l < target/gh-requests.log)" -eq 1
grep -q '/reviews' target/gh-requests.log
! grep -q '/pulls/123/comments' target/gh-requests.log
jq -e '
  .event == "COMMENT"
  and (.comments | length) == 3
  and (.body | contains("3 additional recommendations"))
  and ([.comments[].body | contains("<details><summary>Full RIPR repair card</summary>")] | all)
  and ([.comments[].body | contains("presentation=compact-v1")] | all)
' target/review-payload.json
cp target/review-payload.json target/create-review-payload.json

jq '[
  [
    .comments
    | to_entries[]
    | {
        id: (100 + .key),
        body: .value.body,
        path: .value.path,
        line: .value.line,
        side: .value.side,
        position: 1
      }
  ]
]' target/review-payload.json > target/existing-source.json
PATH="$PWD/target/fake-bin:$PATH" bash target/capture-step.sh
python - <<'PY'
import json
from pathlib import Path

existing = json.loads(Path("target/ripr/review/existing-comments.json").read_text())["comments"]
operations = json.loads(Path("target/ripr/review/comment-publish-plan.json").read_text())["operations"]
planned = {operation["dedupe_key"]: operation["body"] for operation in operations}
assert len(existing) == 3
for comment in existing:
    assert comment["body"] == planned[comment["dedupe_key"]]
PY

jq -n \
  --arg body "$(jq -r '.operations[0].body' target/ripr/review/comment-publish-plan.json)" \
  '[[{
    id: 200,
    body: ($body + "\n\n<!-- ripr:dedupe=ripr:one -->\n"),
    path: "src/lib.rs",
    line: 10,
    side: "RIGHT",
    position: 1
  }]]' > target/existing-source.json
PATH="$PWD/target/fake-bin:$PATH" bash target/capture-step.sh
jq -e '.comments[0].body == "__ripr_legacy_presentation__"' \
  target/ripr/review/existing-comments.json

jq '.summary.publishable = 2
  | .summary.summary_only = 3
  | .operations = [
      (.operations[0] | .operation = "update" | .existing_comment_id = 41),
      (.operations[1] | .operation = "keep" | .existing_comment_id = 42)
    ]' target/ripr/review/comment-publish-plan.json > target/ripr/review/comment-publish-plan.next.json
mv target/ripr/review/comment-publish-plan.next.json target/ripr/review/comment-publish-plan.json
rm -f target/gh-requests.log target/review-payload.json target/update-payload.json
PATH="$PWD/target/fake-bin:$PATH" bash target/publish-step.sh
test "$(wc -l < target/gh-requests.log)" -eq 2
sed -n '1p' target/gh-requests.log | grep -q -- '--method PATCH'
sed -n '2p' target/gh-requests.log | grep -q '/reviews'
jq -e 'has("comments") | not' target/review-payload.json
jq -e '.body | contains("presentation=compact-v1")' target/update-payload.json

jq '.summary.publishable = 1
  | .operations = [(.operations[0] | .operation = "keep" | .existing_comment_id = 41)]' target/ripr/review/comment-publish-plan.json > target/ripr/review/comment-publish-plan.next.json
mv target/ripr/review/comment-publish-plan.next.json target/ripr/review/comment-publish-plan.json
rm -f target/gh-requests.log target/review-payload.json target/update-payload.json
PATH="$PWD/target/fake-bin:$PATH" bash target/publish-step.sh
test ! -e target/gh-requests.log
