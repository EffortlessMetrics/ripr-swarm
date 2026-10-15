//! Native Windows proof for the documented PowerShell recipe. Rust owns the
//! fixture variants and assertions; only the recipe and CLI mock run in pwsh.

use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

enum Expected {
    Open,
    Clear(u64),
    Refused(&'static [&'static str], usize),
    WrongFilter,
    WrongFirstPage,
    WrongCursorGuard,
}

struct Case {
    name: &'static str,
    pages: Value,
    recipe: String,
    cli_exit: i32,
    expected: Expected,
}

#[test]
fn review_conversation_recipe_fixtures() -> Result<(), String> {
    run_fixtures(false)
}

#[test]
fn review_conversation_recipe_fixtures_strict() -> Result<(), String> {
    run_fixtures(true)
}

fn run_fixtures(strict: bool) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "xtask manifest has no workspace parent".to_string())?;
    let reference = root.join(".agents/skills/finish-pr/references");
    let markdown = fs::read_to_string(reference.join("review-conversations.md"))
        .map_err(|error| error.to_string())?
        .replace("\r\n", "\n");
    let recipe = markdown
        .split_once("```powershell\n")
        .and_then(|(_, rest)| rest.split_once("\n```"))
        .map(|(code, _)| code.to_string())
        .ok_or_else(|| "missing executable recipe fence".to_string())?;
    let fixture: Value = serde_json::from_slice(
        &fs::read(reference.join("review-conversations.fixture.json"))
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let pages = fixture["pages"].clone();
    if pages.as_array().is_none_or(|pages| pages.len() != 2) {
        return Err("fixture must supply two pages".to_string());
    }
    let cases = fixture_cases(&pages, &recipe)?;
    if cases.len() != 27 {
        return Err("fixture case inventory changed without updating its denominator".to_string());
    }
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let parent = root.join("target/review-conversation-fixtures");
    fs::create_dir_all(&parent).map_err(|error| error.to_string())?;
    let scratch = parent.join(format!("{}-{stamp}-{strict}", std::process::id()));
    fs::create_dir(&scratch).map_err(|error| error.to_string())?;
    let wrapper = scratch.join("mock-cli.ps1");
    fs::write(&wrapper, MOCK_CLI).map_err(|error| error.to_string())?;
    for (index, case) in cases.iter().enumerate() {
        // Retain small, uniquely owned input witnesses without overwriting old runs.
        let case_root = scratch.join(index.to_string());
        fs::create_dir(&case_root).map_err(|error| error.to_string())?;
        let recipe_path = case_root.join("recipe.ps1");
        let pages_path = case_root.join("pages.json");
        fs::write(&recipe_path, &case.recipe).map_err(|error| error.to_string())?;
        fs::write(
            &pages_path,
            serde_json::to_vec(&case.pages).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let mut args = vec![
            "-NoLogo".to_string(),
            "-NoProfile".to_string(),
            "-NonInteractive".to_string(),
            "-File".to_string(),
            wrapper.to_string_lossy().into_owned(),
            "-RecipePath".to_string(),
            recipe_path.to_string_lossy().into_owned(),
            "-PagesPath".to_string(),
            pages_path.to_string_lossy().into_owned(),
            "-MockExit".to_string(),
            case.cli_exit.to_string(),
        ];
        if strict {
            args.push("-Strict".to_string());
        }
        let output = crate::run::run_output_owned_with_timeout(
            "pwsh",
            &args,
            Duration::from_mins(1),
            "review-conversation fixture recipe",
        )?;
        let result: Value = serde_json::from_str(&output).map_err(|error| {
            format!("{}: invalid fixture protocol: {error}\n{output}", case.name)
        })?;
        check_result(&result, &case.expected)
            .map_err(|error| format!("{} (strict={strict}): {error}", case.name))?;
    }
    Ok(())
}

fn connection(pages: &mut Value, page: usize) -> &mut Value {
    &mut pages[page]["data"]["repository"]["pullRequest"]["reviewThreads"]
}

fn fixture_cases(pages: &Value, recipe: &str) -> Result<Vec<Case>, String> {
    let case = |name, pages, expected| Case {
        name,
        pages,
        recipe: recipe.to_string(),
        cli_exit: 0,
        expected,
    };
    let mut cases = vec![case("two-pages", pages.clone(), Expected::Open)];
    let mut resolved = pages.clone();
    for index in 0..2 {
        for node in connection(&mut resolved, index)["nodes"]
            .as_array_mut()
            .ok_or_else(|| "fixture nodes must be arrays".to_string())?
        {
            node["isResolved"] = json!(true);
        }
    }
    cases.push(case("resolved-only", resolved, Expected::Clear(3)));
    cases.push(case(
        "truncated",
        json!([pages[0].clone()]),
        Expected::Refused(&["Conversation lookup failed (1)"], 2),
    ));
    let mut moved = pages.clone();
    moved[1]["data"]["repository"]["pullRequest"]["headRefOid"] = json!("c".repeat(40));
    cases.push(case(
        "head-moved",
        moved,
        Expected::Refused(&["PR head changed"], 2),
    ));
    let mut errors = pages.clone();
    errors[0]["errors"] = json!([{"message": "partial GraphQL error"}]);
    cases.push(case(
        "partial-error",
        errors,
        Expected::Refused(&["Incomplete or wrong-subject"], 1),
    ));
    let mut inaccessible = pages.clone();
    inaccessible[0]["data"]["repository"] = Value::Null;
    cases.push(case(
        "inaccessible",
        inaccessible,
        Expected::Refused(&["Incomplete or wrong-subject", "pullRequest"], 1),
    ));
    let mut missing_nodes = pages.clone();
    connection(&mut missing_nodes, 0)
        .as_object_mut()
        .ok_or_else(|| "fixture connection must be an object".to_string())?
        .remove("nodes");
    cases.push(case(
        "missing-nodes",
        missing_nodes,
        Expected::Refused(&["Incomplete or wrong-subject", "nodes"], 1),
    ));
    let mut missing_resolution = pages.clone();
    connection(&mut missing_resolution, 0)["nodes"][1]
        .as_object_mut()
        .ok_or_else(|| "fixture thread must be an object".to_string())?
        .remove("isResolved");
    cases.push(case(
        "missing-resolution",
        missing_resolution,
        Expected::Refused(&["Incomplete or duplicate", "isResolved"], 1),
    ));
    let mut duplicate = pages.clone();
    connection(&mut duplicate, 1)["nodes"][0]["id"] = json!("open-outdated");
    cases.push(case(
        "duplicate-id",
        duplicate,
        Expected::Refused(&["Incomplete or duplicate"], 2),
    ));
    let mut missing_link = pages.clone();
    connection(&mut missing_link, 1)["nodes"][0]["comments"]["nodes"][0]["url"] = Value::Null;
    cases.push(case(
        "missing-link",
        missing_link,
        Expected::Refused(&["Unresolved thread has no conversation link"], 2),
    ));
    let mut native_failure = case(
        "native-exit",
        pages.clone(),
        Expected::Refused(&["Conversation lookup failed (1)"], 1),
    );
    native_failure.cli_exit = 1;
    cases.push(native_failure);
    let mut wrong_filter = case("wrong-filter", pages.clone(), Expected::WrongFilter);
    wrong_filter.recipe = recipe.replace(
        "if (-not $conversationThread.isResolved)",
        "if (-not $conversationThread.isResolved -and -not $conversationThread.isOutdated)",
    );
    cases.push(wrong_filter);
    let mut wrong_first_page = case("wrong-first-page", pages.clone(), Expected::WrongFirstPage);
    wrong_first_page.recipe = recipe.replace("while ($conversationMore)", "while ($false)");
    cases.push(wrong_first_page);
    let mut alias = case("case-alias", pages.clone(), Expected::Open);
    alias.recipe = recipe.replace(
        "'EffortlessMetrics/ripr-swarm'",
        "'effortlessmetrics/ripr-swarm'",
    );
    if alias.recipe == recipe {
        return Err("repository input anchor disappeared".to_string());
    }
    cases.push(alias);
    let mut wrong_repo = pages.clone();
    wrong_repo[0]["data"]["repository"]["nameWithOwner"] = json!("wrong/repository");
    cases.push(case(
        "wrong-repository",
        wrong_repo,
        Expected::Refused(&["Incomplete or wrong-subject"], 1),
    ));
    let mut empty = json!([pages[0].clone()]);
    connection(&mut empty, 0)["nodes"] = json!([]);
    connection(&mut empty, 0)["pageInfo"]["hasNextPage"] = json!(false);
    cases.push(case("empty", empty, Expected::Clear(0)));
    for (name, cursor) in [("missing-cursor", Value::Null), ("empty-cursor", json!(""))] {
        let mut invalid = pages.clone();
        connection(&mut invalid, 0)["pageInfo"]["endCursor"] = cursor;
        cases.push(case(
            name,
            invalid,
            Expected::Refused(&["Incomplete or repeated conversation cursor"], 1),
        ));
    }
    let mut repeated = pages.clone();
    connection(&mut repeated, 1)["pageInfo"] =
        json!({"hasNextPage": true, "endCursor": "page-two"});
    cases.push(case(
        "repeated-nonterminal-cursor",
        repeated.clone(),
        Expected::Refused(&["Incomplete or repeated conversation cursor"], 2),
    ));
    let mut wrong_guard = case(
        "wrong-cursor-guard",
        repeated.clone(),
        Expected::WrongCursorGuard,
    );
    wrong_guard.recipe =
        recipe.replace(" -or -not $conversationCursors.Add($conversationNext)", "");
    cases.push(wrong_guard);
    let mut third = repeated[1].clone();
    connection(&mut repeated, 1)["pageInfo"]["endCursor"] = json!("page-three");
    third["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"][0]["id"] =
        json!("third-page");
    repeated
        .as_array_mut()
        .ok_or_else(|| "fixture pages must be an array".to_string())?
        .push(third);
    cases.push(case(
        "cursor-cycle",
        repeated,
        Expected::Refused(&["Incomplete or repeated conversation cursor"], 3),
    ));
    let mut terminal_repeat = pages.clone();
    connection(&mut terminal_repeat, 1)["pageInfo"]["endCursor"] = json!("page-two");
    cases.push(case("terminal-repeat", terminal_repeat, Expected::Open));
    let mut terminal_missing = pages.clone();
    connection(&mut terminal_missing, 1)["pageInfo"] = json!({"hasNextPage": false});
    cases.push(case("terminal-missing", terminal_missing, Expected::Open));
    for (name, cursor) in [
        ("opaque-file", "@not-a-file"),
        ("opaque-null", "null"),
        ("opaque-number", "00123"),
        ("opaque-characters", "quote\"\\`$(not-a-command)\u{00e9}"),
    ] {
        let mut opaque = pages.clone();
        connection(&mut opaque, 0)["pageInfo"]["endCursor"] = json!(cursor);
        cases.push(case(name, opaque, Expected::Open));
    }
    Ok(cases)
}

fn check_result(result: &Value, expected: &Expected) -> Result<(), String> {
    let calls = match expected {
        Expected::Open | Expected::WrongFilter => 2,
        Expected::Clear(0) | Expected::WrongFirstPage => 1,
        Expected::Clear(_) => 2,
        Expected::Refused(_, calls) => *calls,
        Expected::WrongCursorGuard => 3,
    };
    if result["requests"] != json!(calls) {
        return Err(format!("expected exactly {calls} page requests: {result}"));
    }
    if let Expected::Refused(reasons, _) = expected {
        let message = result["error"].as_str().unwrap_or_default();
        if result["ok"] != false
            || result.get("packet").is_some()
            || !reasons.iter().any(|reason| message.contains(*reason))
        {
            return Err(format!("expected named refusal with no packet: {result}"));
        }
        return Ok(());
    }
    if let Expected::WrongCursorGuard = expected {
        if result["ok"] != false
            || result.get("packet").is_some()
            || !result["error"]
                .as_str()
                .is_some_and(|error| error.contains("Conversation lookup failed (1)"))
        {
            return Err(format!(
                "cursor guard removal did not reach the distinct request limit: {result}"
            ));
        }
        return Ok(());
    }
    if result["ok"] != true
        || result["packet"]["observed_head"] != "b".repeat(40)
        || result["packet"]["pr"] != 42
    {
        return Err(format!(
            "expected successful exact-subject packet: {result}"
        ));
    }
    let packet = &result["packet"];
    match expected {
        Expected::Open => validate_open(packet),
        Expected::Clear(inspected) => {
            if packet["threads_inspected"] != *inspected
                || packet["unresolved_count"] != 0
                || !packet["unresolved"].as_array().is_some_and(Vec::is_empty)
                || !packet["next_action"].as_str().is_some_and(|action| {
                    action.contains("zero threads is not review or merge permission")
                })
            {
                return Err(format!("clear inventory lost its obligations: {packet}"));
            }
            Ok(())
        }
        Expected::WrongFilter | Expected::WrongFirstPage => {
            if validate_open(packet).is_ok() || packet["unresolved_count"] != 1 {
                return Err(format!(
                    "packet oracle accepted the wrong collector: {packet}"
                ));
            }
            Ok(())
        }
        Expected::Refused(_, _) => Err("refusal branch was not inspected".to_string()),
        Expected::WrongCursorGuard => Err("cursor control branch was not inspected".to_string()),
    }
}

fn validate_open(packet: &Value) -> Result<(), String> {
    if packet["threads_inspected"] != 3
        || packet["unresolved_count"] != 2
        || packet["unresolved"][0]["id"] != "open-outdated"
        || packet["unresolved"][0]["url"] != "https://example.invalid/outdated"
        || packet["unresolved"][0]["outdated"] != true
        || packet["unresolved"][0]["original_line"] != 20
        || packet["unresolved"][1]["id"] != "open-current"
        || packet["unresolved"][1]["url"] != "https://example.invalid/current"
        || !packet["next_action"]
            .as_str()
            .is_some_and(|action| action.contains("blocked"))
        || !packet["unresolved"][0]["next_action"]
            .as_str()
            .is_some_and(|action| action.contains("verify reply/repair before resolving"))
    {
        return Err(format!(
            "open inventory lost findings, subject or next action: {packet}"
        ));
    }
    Ok(())
}

// This child protocol reports recipe refusals as data. Rust checks those outcomes
// separately from the wrapper's native status; setup/parser failures still fail.
const MOCK_CLI: &str = r#"
param([string]$RecipePath, [string]$PagesPath, [int]$MockExit, [switch]$Strict)
$ErrorActionPreference = 'Stop'
if ($Strict) { Set-StrictMode -Version Latest }
$global:conversationMockPages = @(Get-Content -LiteralPath $PagesPath -Raw | ConvertFrom-Json)
$global:conversationMockExit = $MockExit
$global:conversationMockCalls = 0
function global:gh {
    $index = $global:conversationMockCalls
    $global:conversationMockCalls++
    if ($args[0] -ne 'api' -or $args[1] -ne 'graphql' -or $args -contains '--paginate' -or $args -contains '--slurp') { throw 'Wrong per-page API invocation' }
    $queries = @($args | Where-Object { $_ -like 'query=*' })
    if ($queries.Count -ne 1 -or $queries[0] -notmatch 'reviewThreads\(first: 100, after: \$endCursor\)' -or $queries[0] -notmatch 'pageInfo \{ hasNextPage endCursor \}' -or $queries[0] -match '\bmutation\b') { throw 'Wrong read-only query' }
    $cursors = @()
    for ($i = 0; $i -lt $args.Count; $i++) {
        if ([string]$args[$i] -clike 'endCursor=*') {
            if ($i -eq 0 -or $args[$i - 1] -cne '-f') { throw 'Cursor must use a raw field' }
            $cursors += $args[$i].Substring(10)
        }
    }
    if ($index -eq 0) {
        if ($cursors.Count) { throw 'Unexpected initial cursor' }
    } elseif ($index -le $global:conversationMockPages.Count) {
        $prior = $global:conversationMockPages[$index - 1].data.repository.pullRequest.reviewThreads.pageInfo.endCursor
        if ($cursors.Count -ne 1 -or $cursors[0] -cne $prior) { throw 'Cursor changed in transport' }
    }
    if ($index -ge $global:conversationMockPages.Count) {
        $global:LASTEXITCODE = 1
        return
    }
    $global:LASTEXITCODE = $global:conversationMockExit
    $global:conversationMockPages[$index] | ConvertTo-Json -Depth 20
}
try {
    $raw = & ([scriptblock]::Create((Get-Content -LiteralPath $RecipePath -Raw)))
    $packet = ($raw -join "`n") | ConvertFrom-Json -ErrorAction Stop
    $result = [pscustomobject]@{ok=$true; packet=$packet; requests=$global:conversationMockCalls}
} catch {
    $result = [pscustomobject]@{ok=$false; error=$_.Exception.Message; requests=$global:conversationMockCalls}
} finally {
    Remove-Item Function:\gh
    Remove-Variable conversationMockPages,conversationMockExit,conversationMockCalls -Scope Global
}
$result | ConvertTo-Json -Depth 20
exit 0
"#;
