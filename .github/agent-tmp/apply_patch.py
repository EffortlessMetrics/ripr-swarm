from pathlib import Path

source_path = Path("crates/ripr/src/cli/commands/init.rs")
source = source_path.read_text()

capture_step = "      - name: Capture existing RIPR inline comments"
if source.count(capture_step) != 1:
    raise SystemExit("expected exactly one existing-comment capture step")
capture_start = source.index("          jq '{\n", source.index(capture_step))
capture_end_marker = "            > target/ripr/review/existing-comments.json"
capture_end = source.index(capture_end_marker, capture_start) + len(capture_end_marker)
capture_replacement = r'''          jq '{
            schema_version: "0.1",
            tool: "ripr",
            kind: "pr_inline_comment_existing_comments",
            comments: [
              .[]?[]?
              | select((.body // "") | contains("<!-- ripr:dedupe="))
              | (.body // "") as $body
              | {
                  comment_id: .id,
                  dedupe_key: ($body | capture("<!-- ripr:dedupe=(?<key>[^ ]+)").key),
                  path: .path,
                  line: (.line // .original_line),
                  side: (.side // "RIGHT"),
                  body: (
                    if ($body | contains(" presentation=compact-v1 -->")) then
                      (($body | [capture("<details><summary>Full RIPR repair card</summary>\n\n(?<card>.*)\n\n</details>"; "m").card][0]) // "__ripr_compact_presentation_unreadable__")
                    else
                      "__ripr_legacy_presentation__"
                    end
                  ),
                  outdated: (.position == null and .line == null)
                }
            ]
          }' target/ripr/review/existing-comments.raw.json \
            > target/ripr/review/existing-comments.json'''
source = source[:capture_start] + capture_replacement + source[capture_end:]

publish_step = "      - name: Publish RIPR inline comments"
if source.count(publish_step) != 1:
    raise SystemExit("expected exactly one inline-comment publisher step")
publish_start = source.index(publish_step)
publish_end = source.index("\n      - name: Capture RIPR gate labels", publish_start)
publish_replacement = r'''      - name: Publish RIPR inline comments
        if: always() && github.event_name == 'pull_request' && env.RIPR_COMMENT_MODE == 'inline' && hashFiles('target/ripr/review/comment-publish-plan.json') != ''
        continue-on-error: true
        env:
          GH_TOKEN: ${{ github.token }}
        run: |
          plan=target/ripr/review/comment-publish-plan.json
          if ! jq -e '.summary.safe_to_publish == true' "$plan" >/dev/null; then
            echo "RIPR inline comments were not published because the publish plan is not safe."
            jq -r '.blocked[]? | "- \(.blocked_reason): \(.message)"' "$plan" || true
            exit 0
          fi

          publishable="$(mktemp)"
          jq '
            def captured($regex; $flags): [capture($regex; $flags).value][0] // null;
            def compact_body:
              .body as $full
              | ($full | captured("^### ripr gap: (?<value>[^\n]+)"; "") // "repairable gap") as $gap
              | ($full | captured("\nRepair:\n(?<value>[^\n]+)"; "") // "Follow the bounded repair route in the RIPR artifact.") as $repair
              | ($full | captured("\nVerify:\n`(?<value>[^`]+)`"; "") // "ripr agent verify") as $verify
              | "**ripr: \($gap)** — \($repair)\n\nVerify: `\($verify)`\n\n<details><summary>Full RIPR repair card</summary>\n\n\($full)\n\n</details>\n\n<!-- ripr:dedupe=\(.dedupe_key) presentation=compact-v1 -->";
            [
              .operations[]?
              | select(.safe_to_publish == true)
              | select(.operation == "create" or .operation == "update" or .operation == "keep")
              | . + {published_body: compact_body}
            ]
          ' "$plan" > "$publishable"

          review_body="$(jq -r '
            (.summary.publishable // 0) as $inline
            | ((.summary.summary_only // 0) + ([.skipped[]? | select(.skip_reason == "inline_comment_cap_reached")] | length)) as $additional
            | (.summary.suppressed // 0) as $suppressed
            | (if $inline == 1 then "" else "s" end) as $inline_suffix
            | (if $additional == 1 then "" else "s" end) as $additional_suffix
            | (if $suppressed == 1 then "" else "s" end) as $suppressed_suffix
            | "RIPR surfaced \($inline) line-placed recommendation\($inline_suffix)."
              + (if $additional > 0 then "\n\n\($additional) additional recommendation\($additional_suffix) remain in the generated `target/ripr/review/comments.json` and `target/ripr/review/comments.md` artifacts." else "" end)
              + (if $suppressed > 0 then "\n\n\($suppressed) suppressed recommendation\($suppressed_suffix) remain visible there with reasons." else "" end)
              + "\n\nAdvisory static evidence only; gate authority remains separate."
          ' "$plan")"

          create_count="$(jq '[.[] | select(.operation == "create")] | length' "$publishable")"
          update_count="$(jq '[.[] | select(.operation == "update")] | length' "$publishable")"
          additional_count="$(jq '(.summary.summary_only // 0) + ([.skipped[]? | select(.skip_reason == "inline_comment_cap_reached")] | length)' "$plan")"
          suppressed_count="$(jq '.summary.suppressed // 0' "$plan")"

          jq -c '.[] | select(.operation == "update")' "$publishable" \
            | while IFS= read -r operation; do
                comment_id="$(jq -r '.existing_comment_id' <<< "$operation")"
                dedupe_key="$(jq -r '.dedupe_key' <<< "$operation")"
                body="$(jq -r '.published_body' <<< "$operation")"
                payload="$(mktemp)"
                jq -n --arg body "$body" '{body: $body}' > "$payload"
                gh api --method PATCH "repos/${{ github.repository }}/pulls/comments/$comment_id" --input "$payload" >/dev/null
                echo "Updated RIPR inline comment: $dedupe_key"
              done

          review_required=false
          if [ "$create_count" -gt 0 ] || { [ "$update_count" -gt 0 ] && { [ "$additional_count" -gt 0 ] || [ "$suppressed_count" -gt 0 ]; }; }; then
            review_required=true
          fi
          if [ "$review_required" = true ]; then
            payload="$(mktemp)"
            jq -n \
              --arg body "$review_body" \
              --arg commit_id "${{ github.event.pull_request.head.sha }}" \
              --argjson create_count "$create_count" \
              --slurpfile operations "$publishable" \
              '({
                body: $body,
                event: "COMMENT",
                commit_id: $commit_id
              } + if $create_count > 0 then {
                comments: [
                  $operations[0][]
                  | select(.operation == "create")
                  | {
                      path: .placement.path,
                      line: .placement.line,
                      side: (.placement.side // "RIGHT"),
                      body: .published_body
                    }
                ]
              } else {} end)' > "$payload"
            gh api --method POST "repos/${{ github.repository }}/pulls/${{ github.event.pull_request.number }}/reviews" --input "$payload" >/dev/null
            if [ "$create_count" -gt 0 ]; then
              echo "Created one RIPR review with $create_count inline comment(s)."
            else
              echo "Created one RIPR review summary after $update_count inline comment update(s)."
            fi
          fi

          jq -r '.[] | select(.operation == "keep") | .dedupe_key' "$publishable" \
            | while IFS= read -r dedupe_key; do
                echo "RIPR inline comment already current: $dedupe_key"
              done'''
if '"#' in publish_replacement or '"#' in capture_replacement:
    raise SystemExit("replacement would terminate the Rust raw string")
source = source[:publish_start] + publish_replacement + "\n" + source[publish_end:]
source_path.write_text(source)

test_path = Path("crates/ripr/tests/generated_review_workflow.rs")
test_path.write_text(r'''use std::error::Error;
use std::fs;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn generated_workflow_batches_compact_review_comments() -> Result<(), Box<dyn Error>> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let root = std::env::temp_dir().join(format!(
        "ripr-generated-review-workflow-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root)?;

    let output = Command::new(env!("CARGO_BIN_EXE_ripr"))
        .args(["init", "--root"])
        .arg(&root)
        .args(["--ci", "github"])
        .output()?;
    assert!(
        output.status.success(),
        "ripr init failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let workflow = fs::read_to_string(root.join(".github/workflows/ripr.yml"))?;
    let review_endpoint = "gh api --method POST \"repos/${{ github.repository }}/pulls/${{ github.event.pull_request.number }}/reviews\"";
    let legacy_create_endpoint = "gh api --method POST \"repos/${{ github.repository }}/pulls/${{ github.event.pull_request.number }}/comments\"";
    let update_endpoint = "gh api --method PATCH \"repos/${{ github.repository }}/pulls/comments/$comment_id\"";
    assert_eq!(workflow.matches(review_endpoint).count(), 1);
    assert!(!workflow.contains(legacy_create_endpoint));
    assert!(workflow.contains(update_endpoint));
    assert!(workflow.contains("event: \"COMMENT\""));
    assert!(workflow.contains("comments: ["));
    assert!(workflow.contains("<details><summary>Full RIPR repair card</summary>"));
    assert!(workflow.contains("presentation=compact-v1"));
    assert!(workflow.contains("__ripr_legacy_presentation__"));
    assert!(workflow.contains("__ripr_compact_presentation_unreadable__"));
    assert!(workflow.contains("additional recommendation"));
    assert!(workflow.contains("target/ripr/review/comments.json"));
    assert!(workflow.contains("target/ripr/review/comments.md"));
    assert!(workflow.contains("Created one RIPR review with $create_count inline comment(s)."));

    fs::remove_dir_all(root)?;
    Ok(())
}
''')