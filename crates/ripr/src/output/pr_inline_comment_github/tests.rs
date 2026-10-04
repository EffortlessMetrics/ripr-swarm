use super::{dedupe_key, existing_comments, is_code_span, publish_requests};
use serde_json::json;

#[test]
fn dedupe_key_is_the_shortest_key_that_closes_on_its_line() {
    assert_eq!(dedupe_key("x <!-- ripr:dedupe=a:b -->"), Some("a:b"));
    assert_eq!(
        dedupe_key("<!-- ripr:dedupe=a b presentation=compact-v1 -->"),
        Some("a b")
    );
    // The first close wins, as `.*?` does.
    assert_eq!(dedupe_key("<!-- ripr:dedupe=a -->b -->"), Some("a"));
    // An empty presentation tag is part of the key, not a tag.
    assert_eq!(
        dedupe_key("<!-- ripr:dedupe=k presentation= -->"),
        Some("k presentation=")
    );
    // The key cannot cross a line; a later marker can still match.
    assert_eq!(dedupe_key("<!-- ripr:dedupe=a\nb -->"), None);
    assert_eq!(
        dedupe_key("<!-- ripr:dedupe=a\n<!-- ripr:dedupe=b -->"),
        Some("b")
    );
}

#[test]
fn code_span_lines_need_matching_fences_around_content() {
    assert!(is_code_span("`ripr agent verify`"));
    assert!(is_code_span("``a`b``"));
    assert!(is_code_span("`a``b`"));
    assert!(!is_code_span("``"));
    assert!(!is_code_span("``a`"));
    assert!(!is_code_span("`a``"));
    assert!(!is_code_span("plain"));
}

#[test]
fn only_marked_comments_from_the_workflow_bot_are_existing_cards() {
    let bot = json!({"login": "github-actions[bot]", "type": "Bot"});
    let pages = json!([[
        {"id": 1, "user": bot, "path": "a.rs", "line": 3, "position": 1,
         "body": "lead\n\n<details><summary>Full RIPR repair card</summary>\n\ncard\n\n</details>\n\n<!-- ripr:dedupe=k1 presentation=compact-v1 -->"},
        {"id": 2, "user": bot, "path": "b.rs", "original_line": 9,
         "body": "old\n<!-- ripr:dedupe=k2 -->"},
        {"id": 3, "user": {"login": "mallory", "type": "User"},
         "body": "<!-- ripr:dedupe=k3 -->"},
        {"id": 4, "user": bot, "body": "no marker"},
        "not an object"
    ]]);
    let existing = existing_comments(&pages);
    assert_eq!(
        existing["comments"],
        json!([
            {"comment_id": 1, "dedupe_key": "k1", "path": "a.rs", "line": 3,
             "side": "RIGHT", "body": "card", "outdated": false},
            {"comment_id": 2, "dedupe_key": "k2", "path": "b.rs", "line": 9,
             "side": "RIGHT", "body": "__ripr_legacy_presentation__", "outdated": true}
        ])
    );
}

#[test]
fn an_unsafe_plan_publishes_nothing_and_says_why() {
    let plan = json!({
        "summary": {"safe_to_publish": false},
        "blocked": [{"blocked_reason": "no_token", "message": "line\r\nbreak"}]
    });
    let requests = publish_requests(&plan, "7", "abc");
    assert!(requests.requests.is_empty());
    assert_eq!(
        requests.notes,
        vec![
            "RIPR inline comments were not published because the publish plan is not safe."
                .to_string(),
            "- no_token: line  break".to_string(),
        ]
    );
}

#[test]
fn updates_come_before_one_review_that_creates_the_new_cards() {
    let plan = json!({
        "summary": {"safe_to_publish": true, "publishable": 2, "summary_only": 1},
        "operations": [
            {"operation": "update", "safe_to_publish": true, "existing_comment_id": 41,
             "dedupe_key": "k1", "body": "### ripr gap: G\nRepair:\nR\nVerify:\n`v`"},
            {"operation": "create", "safe_to_publish": true, "dedupe_key": "k2",
             "placement": {"path": "src/a.rs", "line": 5}, "body": "card"},
            {"operation": "create", "safe_to_publish": false, "dedupe_key": "k3", "body": "x"},
            {"operation": "keep", "safe_to_publish": true, "dedupe_key": "k4\nx", "body": "y"}
        ]
    });
    let requests = publish_requests(&plan, "7", "abc");
    let calls: Vec<_> = requests
        .requests
        .iter()
        .map(|request| (request.method, request.endpoint.as_str()))
        .collect();
    assert_eq!(
        calls,
        vec![("PATCH", "pulls/comments/41"), ("POST", "pulls/7/reviews")]
    );
    assert_eq!(
        requests.requests[0].payload["body"],
        "**ripr: G** — R\n\nVerify: `v`\n\n<details><summary>Full RIPR repair card</summary>\n\n### ripr gap: G\nRepair:\nR\nVerify:\n`v`\n\n</details>\n\n<!-- ripr:dedupe=k1 presentation=compact-v1 -->"
    );
    let review = &requests.requests[1].payload;
    assert_eq!(review["commit_id"], "abc");
    assert_eq!(review["event"], "COMMENT");
    assert_eq!(review["comments"][0]["path"], "src/a.rs");
    assert_eq!(review["comments"][0]["side"], "RIGHT");
    assert_eq!(review["comments"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        requests.requests[1].message,
        "Created one RIPR review with 1 inline comment(s)."
    );
    assert_eq!(
        requests.notes,
        vec!["RIPR inline comment already current: k4 x".to_string()]
    );
}
