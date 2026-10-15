//! Native Windows proof for the documented PowerShell recipe. Rust owns the
//! fixture variants and assertions; only the recipe and CLI mock run in pwsh.

use serde_json::{Value, json};
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

enum Expected {
    Open,
    Clear(u64),
    Refused(&'static [&'static str]),
    WrongFilter,
    WrongFirstPage,
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
    if !pages.as_array().is_some_and(|pages| pages.len() == 2) {
        return Err("fixture must supply two pages".to_string());
    }
    let cases = fixture_cases(&pages, &recipe)?;
    if cases.len() != 16 {
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
            Duration::from_secs(60),
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
        Expected::Refused(&["Incomplete conversation pagination"]),
    ));
    let mut moved = pages.clone();
    moved[1]["data"]["repository"]["pullRequest"]["headRefOid"] = json!("c".repeat(40));
    cases.push(case(
        "head-moved",
        moved,
        Expected::Refused(&["PR head changed"]),
    ));
    let mut errors = pages.clone();
    errors[0]["errors"] = json!([{"message": "partial GraphQL error"}]);
    cases.push(case(
        "partial-error",
        errors,
        Expected::Refused(&["Incomplete or wrong-subject"]),
    ));
    let mut inaccessible = pages.clone();
    inaccessible[0]["data"]["repository"] = Value::Null;
    cases.push(case(
        "inaccessible",
        inaccessible,
        Expected::Refused(&["Incomplete or wrong-subject", "pullRequest"]),
    ));
    let mut missing_nodes = pages.clone();
    connection(&mut missing_nodes, 0)
        .as_object_mut()
        .ok_or_else(|| "fixture connection must be an object".to_string())?
        .remove("nodes");
    cases.push(case(
        "missing-nodes",
        missing_nodes,
        Expected::Refused(&["Incomplete or wrong-subject", "nodes"]),
    ));
    let mut missing_resolution = pages.clone();
    connection(&mut missing_resolution, 0)["nodes"][1]
        .as_object_mut()
        .ok_or_else(|| "fixture thread must be an object".to_string())?
        .remove("isResolved");
    cases.push(case(
        "missing-resolution",
        missing_resolution,
        Expected::Refused(&["Incomplete or duplicate", "isResolved"]),
    ));
    let mut duplicate = pages.clone();
    connection(&mut duplicate, 1)["nodes"][0]["id"] = json!("open-outdated");
    cases.push(case(
        "duplicate-id",
        duplicate,
        Expected::Refused(&["Incomplete or duplicate"]),
    ));
    let mut missing_link = pages.clone();
    connection(&mut missing_link, 1)["nodes"][0]["comments"]["nodes"][0]["url"] = Value::Null;
    cases.push(case(
        "missing-link",
        missing_link,
        Expected::Refused(&["Unresolved thread has no conversation link"]),
    ));
    let mut native_failure = case(
        "native-exit",
        pages.clone(),
        Expected::Refused(&["Conversation lookup failed (1)"]),
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
    wrong_first_page.recipe = recipe.replace(
        "for ($conversationIndex = 0;",
        "$conversationPages = @($conversationPages[0]); $conversationPages[0].data.repository.pullRequest.reviewThreads.pageInfo.hasNextPage = $false; for ($conversationIndex = 0;",
    );
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
        Expected::Refused(&["Incomplete or wrong-subject"]),
    ));
    let mut empty = json!([pages[0].clone()]);
    connection(&mut empty, 0)["nodes"] = json!([]);
    connection(&mut empty, 0)["pageInfo"]["hasNextPage"] = json!(false);
    cases.push(case("empty", empty, Expected::Clear(0)));
    Ok(cases)
}

fn check_result(result: &Value, expected: &Expected) -> Result<(), String> {
    if let Expected::Refused(reasons) = expected {
        let message = result["error"].as_str().unwrap_or_default();
        if result["ok"] != false
            || result.get("packet").is_some()
            || !reasons.iter().any(|reason| message.contains(*reason))
        {
            return Err(format!("expected named refusal with no packet: {result}"));
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
        Expected::Refused(_) => Err("refusal branch was not inspected".to_string()),
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
$global:conversationMockRaw = Get-Content -LiteralPath $PagesPath -Raw
$global:conversationMockExit = $MockExit
function global:gh {
    if ($args[0] -ne 'api' -or $args[1] -ne 'graphql' -or $args -notcontains '--paginate' -or $args -notcontains '--slurp') { throw 'Wrong API/pagination invocation' }
    $queries = @($args | Where-Object { $_ -like 'query=*' })
    if ($queries.Count -ne 1 -or $queries[0] -notmatch 'reviewThreads\(first: 100, after: \$endCursor\)' -or $queries[0] -notmatch 'pageInfo \{ hasNextPage endCursor \}' -or $queries[0] -match '\bmutation\b') { throw 'Wrong read-only query' }
    $global:LASTEXITCODE = $global:conversationMockExit
    $global:conversationMockRaw
}
try {
    $raw = & ([scriptblock]::Create((Get-Content -LiteralPath $RecipePath -Raw)))
    $packet = ($raw -join "`n") | ConvertFrom-Json -ErrorAction Stop
    $result = [pscustomobject]@{ok=$true; packet=$packet}
} catch {
    $result = [pscustomobject]@{ok=$false; error=$_.Exception.Message}
} finally {
    Remove-Item Function:\gh
    Remove-Variable conversationMockRaw,conversationMockExit -Scope Global
}
$result | ConvertTo-Json -Depth 20
exit 0
"#;
