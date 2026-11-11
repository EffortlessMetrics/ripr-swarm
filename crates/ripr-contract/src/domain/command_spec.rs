//! Typed, producer-owned command descriptions.
//!
//! A [`CommandSpec`] describes a route for a consumer. It is not permission
//! to execute that route. In particular, the human display string is never
//! the source from which a consumer may reconstruct `program` or `args`.

use std::fmt;
use std::path::{Component, Path};

const MAX_DISPLAY_LENGTH: usize = 512;
const MAX_TIMEOUT_MS: u64 = 3_600_000;

/// The Python producer's pytest display, shared with its rooted projection.
/// `node` is the qualified test name with `::` separators, not a shell token.
pub fn python_pytest_verify_display(path: &str, node: &str, windows: bool) -> Option<String> {
    let selector = if windows {
        shell_quote_file_arg_for_host(&format!("{path}::{node}"), true)?
    } else {
        format!("{}::{node}", shell_quote_file_arg_for_host(path, false)?)
    };
    Some(format!(
        "{} {selector}",
        crate::domain::PYTEST_VERIFY_PROGRAM
    ))
}

pub fn python_unittest_verify_display(
    path: &str,
    qualified: &str,
    windows: bool,
) -> Option<String> {
    let module = unittest_module_for_path(path)?;
    let selector = if windows {
        shell_quote_file_arg_for_host(&format!("{module}.{qualified}"), true)?
    } else {
        format!(
            "{}.{qualified}",
            shell_quote_file_arg_for_host(&module, false)?
        )
    };
    Some(format!("python -m unittest {selector}"))
}

fn unittest_module_for_path(path: &str) -> Option<String> {
    let module_path = path.strip_suffix(".py").unwrap_or(path);
    // unittest splits every dot as a module separator. A literal path dot
    // can therefore select a different file, even when shell quoting is
    // correct. Keep that unresolved identity out of verification routes.
    if module_path.contains('.') {
        return None;
    }
    Some(module_path.replace(['/', '\\'], "."))
}

/// Quote one path or module token for the host's default shell grammar.
///
/// Suggested verify commands are text an agent may paste into a shell, so the
/// quoting must match the host's grammar or the command fails for exactly the
/// paths that need quoting (#8752). A relative path made of ASCII letters,
/// digits, `.`, `_`, `/`, `-` stays readable (`:` too on Windows, where it is
/// not a metachar and the pytest `::` separator must not force quotes); any
/// other byte is quoted so spaces, unicode, and shell metacharacters reach
/// the test runner intact. On POSIX hosts the quote is single-quote escaping
/// (`'\''`), so `$()`, backticks, spaces, and quotes are not expanded. On
/// Windows hosts the quote is double quotes — the common denominator of cmd
/// and PowerShell for literal text — but both shells still expand some bytes
/// inside double quotes (PowerShell: `$()`, `$var`, and backtick escapes;
/// cmd: `%VAR%`). A token carrying any of those bytes gets no paste-ready
/// command at all: the verify route stays silent rather than emitting a
/// command whose paste executes the file name. An embedded `"` in a
/// quotable token is doubled per the PowerShell grammar; cmd has no
/// representation for an embedded quote, which stays a documented limit.
/// Stored node ids and test-file paths stay the raw spelling; only the
/// command text is quoted. This does not run the command.
fn shell_quote_file_arg_for_host(file_str: &str, windows: bool) -> Option<String> {
    let in_safe_class = |c: char| {
        c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '-') || (windows && c == ':')
    };
    if !file_str.is_empty() && file_str.chars().all(in_safe_class) {
        return Some(file_str.to_string());
    }
    if windows {
        if file_str.contains(['$', '`', '%']) {
            return None;
        }
        return Some(format!("\"{}\"", file_str.replace('"', "\"\"")));
    }
    Some(format!("'{}'", file_str.replace('\'', "'\\''")))
}

/// Recognize the closed verification-provenance set of actionable packets.
///
/// RIPR-SPEC-0057 admits an existing canonical command or the producer's
/// bounded typed-related-test fallback. A recognized label does not
/// authenticate its producer or grant command-execution authority.
pub fn actionable_verify_source_is_supported(source: &str) -> bool {
    matches!(
        source,
        "canonical_item.verify_command" | "related_test_or_observer.name"
    )
}

/// Identify the broad snapshot comparison excluded from actionable verification.
///
/// This is the existing RIPR-SPEC-0057 display-policy predicate, shared by
/// projection and passive admission. It is not a shell parser, a general cost
/// estimate, or permission to execute any other command.
pub fn verify_command_is_unbounded_repo_exposure_snapshot_compare(command: &str) -> bool {
    let normalized = command_tokens(&command.trim().replace('\\', "/")).join(" ");
    normalized.contains("ripr agent verify")
        && normalized.contains("--before")
        && normalized.contains("--after")
        && normalized.contains("repo-exposure.json")
}

/// Decode the existing passive command display words for shared policy checks.
///
/// This preserves the LSP token interpretation; it does not execute a command
/// or implement a general shell parser.
pub fn command_tokens(command: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for character in command.chars() {
        match (quote, character) {
            (Some(active), value) if value == active => quote = None,
            (None, '"' | '\'') => quote = Some(character),
            (None, value) if value.is_whitespace() => {
                if !current.is_empty() {
                    tokens.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(character),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CommandSpec {
    pub schema_version: String,
    pub command_id: String,
    pub role: CommandRole,
    pub execution_mode: CommandExecutionMode,
    pub program: String,
    pub args: Vec<String>,
    #[serde(rename = "working_directory", alias = "cwd")]
    pub cwd: String,
    pub env_set: Vec<EnvironmentAssignment>,
    pub env_passthrough: Vec<String>,
    #[serde(rename = "environment", alias = "environment_policy")]
    pub environment_policy: EnvironmentPolicy,
    pub stdin: StdinPolicy,
    pub timeout_ms: u64,
    pub cancellation: CancellationPolicy,
    #[serde(rename = "network", alias = "network_policy")]
    pub network_policy: NetworkPolicy,
    pub expected_result_parser: ExpectedResultParser,
    pub expected_exit_codes: Vec<i32>,
    pub expected_writes: Vec<String>,
    pub cost_class: CommandCostClass,
    pub platforms: Vec<CommandPlatform>,
    #[serde(rename = "human_display", alias = "display")]
    pub display: String,
    pub authority_boundary: CommandAuthorityBoundary,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandRole {
    Verify,
    Receipt,
    Regeneration,
    Inspection,
    TargetedRerun,
    /// Start one repair attempt for a single bound seam (#6305). The
    /// task-first repair command offers this route only when its producer
    /// binds exactly one eligible subject.
    RepairStart,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandExecutionMode {
    Direct,
    ShellRequired,
    Manual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StdinPolicy {
    Null,
    Declared,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationPolicy {
    Allowed,
    NotAllowed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    Forbidden,
    NotRequired,
    Declared,
    Unrestricted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentPolicy {
    Clean,
    Declared,
    Inherited,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EnvironmentAssignment {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedResultParser {
    ExitCode,
    DeclaredJson,
    DeclaredText,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandCostClass {
    Unknown,
    Pilot,
    Check,
    ProjectionOnly,
    BoundedStaticAnalysis,
    FullRepoAnalysis,
    CompileOrTest,
    RuntimeMutationOrImport,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandPlatform {
    Linux,
    Macos,
    Windows,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandAuthorityBoundary {
    VerificationRouteOnly,
    ReceiptRouteOnly,
    RegenerationRouteOnly,
    InspectionRouteOnly,
    TargetedRerunRouteOnly,
    RepairStartRouteOnly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandSpecValidationError {
    EmptyField(&'static str),
    EmbeddedNul(&'static str),
    InvalidSchemaVersion(String),
    InvalidRelativePath { field: &'static str, value: String },
    DisplayTooLong,
    InvalidTimeout(u64),
    EmptyExpectedExitCodes,
    EmptyPlatforms,
    InvalidEnvironmentName(String),
    AuthorityRoleMismatch,
}

impl fmt::Display for CommandSpecValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyField(field) => write!(formatter, "{field} must not be empty"),
            Self::EmbeddedNul(field) => write!(formatter, "{field} must not contain NUL"),
            Self::InvalidSchemaVersion(version) => {
                write!(formatter, "unsupported command schema version {version:?}")
            }
            Self::InvalidRelativePath { field, value } => {
                write!(formatter, "{field} must be a root-relative path: {value:?}")
            }
            Self::DisplayTooLong => {
                write!(
                    formatter,
                    "display must be at most {MAX_DISPLAY_LENGTH} characters"
                )
            }
            Self::InvalidTimeout(timeout_ms) => {
                write!(
                    formatter,
                    "timeout must be between 1 and {MAX_TIMEOUT_MS} ms, got {timeout_ms}"
                )
            }
            Self::EmptyExpectedExitCodes => {
                write!(formatter, "expected_exit_codes must not be empty")
            }
            Self::EmptyPlatforms => write!(formatter, "platforms must not be empty"),
            Self::InvalidEnvironmentName(name) => {
                write!(formatter, "invalid environment variable name {name:?}")
            }
            Self::AuthorityRoleMismatch => {
                write!(formatter, "authority boundary does not match command role")
            }
        }
    }
}

impl std::error::Error for CommandSpecValidationError {}

impl CommandSpec {
    pub const SCHEMA_VERSION: &'static str = "1";

    /// True when both specs describe the same executable route: every field
    /// except the human display (#3999). A current producer's display names
    /// the concrete selected checkout root, so two equivalent checkouts — or
    /// a portable `--root .` display and its root-bound rendering — share
    /// one route while their displays differ. The display is never execution
    /// authority (see the module docs), so route identity excludes it.
    pub fn same_route(&self, other: &Self) -> bool {
        // Exhaustive destructuring: a field added to `CommandSpec` fails to
        // compile here until it is classified as route identity or display.
        let Self {
            schema_version,
            command_id,
            role,
            execution_mode,
            program,
            args,
            cwd,
            env_set,
            env_passthrough,
            environment_policy,
            stdin,
            timeout_ms,
            cancellation,
            network_policy,
            expected_result_parser,
            expected_exit_codes,
            expected_writes,
            cost_class,
            platforms,
            display: _,
            authority_boundary,
        } = self;
        *schema_version == other.schema_version
            && *command_id == other.command_id
            && *role == other.role
            && *execution_mode == other.execution_mode
            && *program == other.program
            && *args == other.args
            && *cwd == other.cwd
            && *env_set == other.env_set
            && *env_passthrough == other.env_passthrough
            && *environment_policy == other.environment_policy
            && *stdin == other.stdin
            && *timeout_ms == other.timeout_ms
            && *cancellation == other.cancellation
            && *network_policy == other.network_policy
            && *expected_result_parser == other.expected_result_parser
            && *expected_exit_codes == other.expected_exit_codes
            && *expected_writes == other.expected_writes
            && *cost_class == other.cost_class
            && *platforms == other.platforms
            && *authority_boundary == other.authority_boundary
    }

    /// Validate fields that must be true before a producer-owned route can be
    /// advertised to a machine consumer. This does not check executable
    /// availability and does not authorize process execution.
    pub fn validate(&self) -> Result<(), CommandSpecValidationError> {
        if self.schema_version != Self::SCHEMA_VERSION {
            return Err(CommandSpecValidationError::InvalidSchemaVersion(
                self.schema_version.clone(),
            ));
        }
        validate_non_empty("command_id", &self.command_id)?;
        validate_non_empty("program", &self.program)?;
        validate_non_empty("display", &self.display)?;
        validate_no_nul("command_id", &self.command_id)?;
        validate_no_nul("program", &self.program)?;
        validate_no_nul("display", &self.display)?;
        for argument in &self.args {
            validate_no_nul("args", argument)?;
        }
        validate_root_relative_path("cwd", &self.cwd)?;
        for path in &self.expected_writes {
            validate_root_relative_path("expected_writes", path)?;
        }
        if self.timeout_ms == 0 || self.timeout_ms > MAX_TIMEOUT_MS {
            return Err(CommandSpecValidationError::InvalidTimeout(self.timeout_ms));
        }
        if self.expected_exit_codes.is_empty() {
            return Err(CommandSpecValidationError::EmptyExpectedExitCodes);
        }
        if self.platforms.is_empty() {
            return Err(CommandSpecValidationError::EmptyPlatforms);
        }
        for assignment in &self.env_set {
            validate_environment_name(&assignment.name)?;
            validate_no_nul("environment value", &assignment.value)?;
        }
        for name in &self.env_passthrough {
            validate_environment_name(name)?;
        }
        if !authority_matches_role(self.role, self.authority_boundary) {
            return Err(CommandSpecValidationError::AuthorityRoleMismatch);
        }
        Ok(())
    }
}

fn validate_non_empty(field: &'static str, value: &str) -> Result<(), CommandSpecValidationError> {
    if value.trim().is_empty() {
        return Err(CommandSpecValidationError::EmptyField(field));
    }
    if field == "display" && value.chars().count() > MAX_DISPLAY_LENGTH {
        return Err(CommandSpecValidationError::DisplayTooLong);
    }
    Ok(())
}

fn validate_no_nul(field: &'static str, value: &str) -> Result<(), CommandSpecValidationError> {
    if value.contains('\0') {
        Err(CommandSpecValidationError::EmbeddedNul(field))
    } else {
        Ok(())
    }
}

fn validate_root_relative_path(
    field: &'static str,
    value: &str,
) -> Result<(), CommandSpecValidationError> {
    validate_no_nul(field, value)?;
    if value.trim().is_empty() {
        return Err(CommandSpecValidationError::EmptyField(field));
    }
    let path = Path::new(value);
    let has_drive_prefix = value.as_bytes().get(1) == Some(&b':');
    let has_host_independent_escape = value.starts_with('/')
        || value.starts_with('\\')
        || value.split(['/', '\\']).any(|component| component == "..");
    let safe = !path.is_absolute()
        && !has_drive_prefix
        && !has_host_independent_escape
        && path.components().all(|component| {
            !matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        });
    if safe {
        Ok(())
    } else {
        Err(CommandSpecValidationError::InvalidRelativePath {
            field,
            value: value.to_string(),
        })
    }
}

fn validate_environment_name(name: &str) -> Result<(), CommandSpecValidationError> {
    if name.is_empty()
        || name.contains('\0')
        || name.contains('=')
        || name.chars().any(char::is_whitespace)
    {
        Err(CommandSpecValidationError::InvalidEnvironmentName(
            name.to_string(),
        ))
    } else {
        Ok(())
    }
}

fn authority_matches_role(role: CommandRole, boundary: CommandAuthorityBoundary) -> bool {
    matches!(
        (role, boundary),
        (
            CommandRole::Verify,
            CommandAuthorityBoundary::VerificationRouteOnly
        ) | (
            CommandRole::Receipt,
            CommandAuthorityBoundary::ReceiptRouteOnly
        ) | (
            CommandRole::Regeneration,
            CommandAuthorityBoundary::RegenerationRouteOnly
        ) | (
            CommandRole::Inspection,
            CommandAuthorityBoundary::InspectionRouteOnly
        ) | (
            CommandRole::TargetedRerun,
            CommandAuthorityBoundary::TargetedRerunRouteOnly
        ) | (
            CommandRole::RepairStart,
            CommandAuthorityBoundary::RepairStartRouteOnly
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verify_spec() -> CommandSpec {
        CommandSpec {
            schema_version: "1".to_string(),
            command_id: "cmd:verify:pricing".to_string(),
            role: CommandRole::Verify,
            execution_mode: CommandExecutionMode::Direct,
            program: "cargo".to_string(),
            args: vec![
                "test".to_string(),
                "-p".to_string(),
                "pricing crate".to_string(),
                "--".to_string(),
                "--exact".to_string(),
            ],
            cwd: ".".to_string(),
            env_set: vec![EnvironmentAssignment {
                name: "RIPR_FIXTURE".to_string(),
                value: "boundary value".to_string(),
            }],
            env_passthrough: vec!["CARGO_HOME".to_string()],
            environment_policy: EnvironmentPolicy::Declared,
            stdin: StdinPolicy::Null,
            timeout_ms: 120_000,
            cancellation: CancellationPolicy::Allowed,
            network_policy: NetworkPolicy::Forbidden,
            expected_result_parser: ExpectedResultParser::ExitCode,
            expected_exit_codes: vec![0],
            expected_writes: vec!["target/**".to_string()],
            cost_class: CommandCostClass::CompileOrTest,
            platforms: vec![CommandPlatform::Linux, CommandPlatform::Windows],
            display: "cargo test -p 'pricing crate' -- --exact".to_string(),
            authority_boundary: CommandAuthorityBoundary::VerificationRouteOnly,
        }
    }

    #[test]
    fn same_route_ignores_only_the_display() {
        let base = verify_spec();
        let mut redisplayed = verify_spec();
        redisplayed.display = "cargo test -p 'pricing crate' -- --exact # /abs/root".to_string();
        assert!(base.same_route(&redisplayed));

        let mut other_args = verify_spec();
        other_args.args.push("--nocapture".to_string());
        assert!(!base.same_route(&other_args));
        let mut other_cwd = verify_spec();
        other_cwd.cwd = "crates".to_string();
        assert!(!base.same_route(&other_cwd));
        let mut other_writes = verify_spec();
        other_writes.expected_writes.clear();
        assert!(!base.same_route(&other_writes));
    }

    #[test]
    fn invalid_paths_and_program_values_fail_closed() -> Result<(), String> {
        let mut cases = Vec::new();
        let mut traversal = verify_spec();
        traversal.cwd = "../outside".to_string();
        cases.push(traversal.validate());
        let mut absolute = verify_spec();
        let drive_letter = char::from(b'C');
        absolute.expected_writes = vec![format!("{drive_letter}:/outside")];
        cases.push(absolute.validate());
        let mut nul_cwd = verify_spec();
        nul_cwd.cwd = "work\0dir".to_string();
        cases.push(nul_cwd.validate());
        let mut nul_write = verify_spec();
        nul_write.expected_writes = vec!["out\0/**".to_string()];
        cases.push(nul_write.validate());
        let mut blank_program = verify_spec();
        blank_program.program = " ".to_string();
        cases.push(blank_program.validate());
        let mut nul_argument = verify_spec();
        nul_argument.args.push("bad\0arg".to_string());
        cases.push(nul_argument.validate());
        if cases.iter().any(Result::is_ok) {
            return Err("malformed command spec was accepted".to_string());
        }
        Ok(())
    }

    #[test]
    fn shell_and_manual_modes_remain_explicit() -> Result<(), String> {
        let mut shell = verify_spec();
        shell.execution_mode = CommandExecutionMode::ShellRequired;
        shell.validate().map_err(|error| error.to_string())?;
        let mut manual = verify_spec();
        manual.execution_mode = CommandExecutionMode::Manual;
        manual.validate().map_err(|error| error.to_string())?;
        Ok(())
    }

    #[test]
    fn mismatched_authority_is_rejected() -> Result<(), String> {
        let mut receipt = verify_spec();
        receipt.role = CommandRole::Receipt;
        if !matches!(
            receipt.validate(),
            Err(CommandSpecValidationError::AuthorityRoleMismatch)
        ) {
            return Err("a verify authority was accepted for a receipt command".to_string());
        }
        Ok(())
    }

    type Refusal = CommandSpecValidationError;

    /// The verify fixture with one edit applied, validated.
    fn validated(edit: impl FnOnce(&mut CommandSpec)) -> Result<(), Refusal> {
        let mut spec = verify_spec();
        edit(&mut spec);
        spec.validate()
    }

    #[test]
    fn the_unedited_fixture_is_a_valid_route() {
        assert_eq!(validated(|_| {}), Ok(()));
    }

    #[test]
    fn only_the_current_schema_version_is_a_route() {
        assert_eq!(
            validated(|spec| spec.schema_version = "2".to_string()),
            Err(Refusal::InvalidSchemaVersion("2".to_string()))
        );
        // The version is read first: a consumer cannot interpret the other
        // fields of a schema it does not know.
        assert_eq!(
            validated(|spec| {
                spec.schema_version = String::new();
                spec.program = String::new();
            }),
            Err(Refusal::InvalidSchemaVersion(String::new()))
        );
    }

    #[test]
    fn a_blank_identity_program_or_display_is_refused_by_name() {
        assert_eq!(
            validated(|spec| spec.command_id = "\t".to_string()),
            Err(Refusal::EmptyField("command_id"))
        );
        assert_eq!(
            validated(|spec| spec.program = String::new()),
            Err(Refusal::EmptyField("program"))
        );
        assert_eq!(
            validated(|spec| spec.display = "  ".to_string()),
            Err(Refusal::EmptyField("display"))
        );
        // Arguments are data: an empty argument is a legitimate argv entry.
        assert_eq!(validated(|spec| spec.args.push(String::new())), Ok(()));
    }

    #[test]
    fn a_nul_is_refused_in_every_text_field_that_reaches_a_process() {
        let nul = |text: &str| format!("{text}\0tail");
        assert_eq!(
            validated(|spec| spec.command_id = nul("cmd")),
            Err(Refusal::EmbeddedNul("command_id"))
        );
        assert_eq!(
            validated(|spec| spec.program = nul("cargo")),
            Err(Refusal::EmbeddedNul("program"))
        );
        assert_eq!(
            validated(|spec| spec.display = nul("cargo test")),
            Err(Refusal::EmbeddedNul("display"))
        );
        assert_eq!(
            validated(|spec| spec.args.push(nul("--exact"))),
            Err(Refusal::EmbeddedNul("args"))
        );
        assert_eq!(
            validated(|spec| spec.cwd = nul("crates")),
            Err(Refusal::EmbeddedNul("cwd"))
        );
        assert_eq!(
            validated(|spec| spec.expected_writes.push(nul("target"))),
            Err(Refusal::EmbeddedNul("expected_writes"))
        );
        assert_eq!(
            validated(|spec| spec.env_set[0].value = nul("value")),
            Err(Refusal::EmbeddedNul("environment value"))
        );
    }

    #[test]
    fn the_display_limit_counts_characters_and_admits_exactly_the_maximum() {
        // Two-byte characters: a byte count would refuse the admitted length.
        assert_eq!(
            validated(|spec| spec.display = "é".repeat(MAX_DISPLAY_LENGTH)),
            Ok(())
        );
        assert_eq!(
            validated(|spec| spec.display = "é".repeat(MAX_DISPLAY_LENGTH + 1)),
            Err(Refusal::DisplayTooLong)
        );
    }

    #[test]
    fn a_path_that_can_leave_the_root_is_refused_with_its_field_and_value() {
        let escapes = [
            "..",
            "../outside",
            "crates/../../outside",
            "crates\\..\\outside",
            "/etc",
            "\\\\server\\share",
            "\\outside",
            "C:/outside",
            "C:outside",
            "c:\\outside",
        ];
        for escape in escapes {
            assert_eq!(
                validated(|spec| spec.cwd = escape.to_string()),
                Err(Refusal::InvalidRelativePath {
                    field: "cwd",
                    value: escape.to_string(),
                }),
                "cwd {escape:?}"
            );
            // A later write is held to the same rule as the first.
            assert_eq!(
                validated(|spec| spec.expected_writes.push(escape.to_string())),
                Err(Refusal::InvalidRelativePath {
                    field: "expected_writes",
                    value: escape.to_string(),
                }),
                "expected_writes {escape:?}"
            );
        }
        assert_eq!(
            validated(|spec| spec.cwd = " ".to_string()),
            Err(Refusal::EmptyField("cwd"))
        );
        assert_eq!(
            validated(|spec| spec.expected_writes.push(String::new())),
            Err(Refusal::EmptyField("expected_writes"))
        );
    }

    #[test]
    fn a_root_relative_path_is_admitted_even_when_it_only_looks_like_an_escape() {
        for inside in [
            ".",
            "crates/ripr",
            "crates\\ripr",
            "./target/ripr",
            "target/**",
            "notes..md",
            "..hidden/file",
            "a/b..c/d",
        ] {
            assert_eq!(
                validated(|spec| spec.cwd = inside.to_string()),
                Ok(()),
                "cwd {inside:?}"
            );
            assert_eq!(
                validated(|spec| spec.expected_writes = vec![inside.to_string()]),
                Ok(()),
                "expected_writes {inside:?}"
            );
        }
        assert_eq!(validated(|spec| spec.expected_writes.clear()), Ok(()));
    }

    #[test]
    fn the_timeout_is_bounded_on_both_sides() {
        assert_eq!(
            validated(|spec| spec.timeout_ms = 0),
            Err(Refusal::InvalidTimeout(0))
        );
        assert_eq!(validated(|spec| spec.timeout_ms = 1), Ok(()));
        assert_eq!(validated(|spec| spec.timeout_ms = MAX_TIMEOUT_MS), Ok(()));
        assert_eq!(
            validated(|spec| spec.timeout_ms = MAX_TIMEOUT_MS + 1),
            Err(Refusal::InvalidTimeout(MAX_TIMEOUT_MS + 1))
        );
    }

    #[test]
    fn a_route_names_at_least_one_expected_exit_code_and_platform() {
        assert_eq!(
            validated(|spec| spec.expected_exit_codes.clear()),
            Err(Refusal::EmptyExpectedExitCodes)
        );
        assert_eq!(
            validated(|spec| spec.platforms.clear()),
            Err(Refusal::EmptyPlatforms)
        );
    }

    #[test]
    fn an_environment_name_that_cannot_be_one_variable_is_refused() {
        for name in [
            "",
            "A=B",
            "=",
            "WITH SPACE",
            "TAB\tBED",
            "NUL\0NAME",
            "LINE\n",
        ] {
            assert_eq!(
                validated(|spec| spec.env_set[0].name = name.to_string()),
                Err(Refusal::InvalidEnvironmentName(name.to_string())),
                "env_set {name:?}"
            );
            assert_eq!(
                validated(|spec| spec.env_passthrough.push(name.to_string())),
                Err(Refusal::InvalidEnvironmentName(name.to_string())),
                "env_passthrough {name:?}"
            );
        }
        // An assigned value is data and may be empty or hold `=`.
        assert_eq!(
            validated(|spec| spec.env_set[0].value = "a=b c".to_string()),
            Ok(())
        );
        assert_eq!(validated(|spec| spec.env_set[0].value.clear()), Ok(()));
    }

    #[test]
    fn each_role_is_admitted_with_its_own_authority_and_no_other() {
        let roles = [
            CommandRole::Verify,
            CommandRole::Receipt,
            CommandRole::Regeneration,
            CommandRole::Inspection,
            CommandRole::TargetedRerun,
            CommandRole::RepairStart,
        ];
        let boundaries = [
            CommandAuthorityBoundary::VerificationRouteOnly,
            CommandAuthorityBoundary::ReceiptRouteOnly,
            CommandAuthorityBoundary::RegenerationRouteOnly,
            CommandAuthorityBoundary::InspectionRouteOnly,
            CommandAuthorityBoundary::TargetedRerunRouteOnly,
            CommandAuthorityBoundary::RepairStartRouteOnly,
        ];
        // The two lists are in matching order: the diagonal is the whole
        // admitted set.
        for (role_at, role) in roles.into_iter().enumerate() {
            for (boundary_at, boundary) in boundaries.into_iter().enumerate() {
                let expected = if role_at == boundary_at {
                    Ok(())
                } else {
                    Err(Refusal::AuthorityRoleMismatch)
                };
                assert_eq!(
                    validated(|spec| {
                        spec.role = role;
                        spec.authority_boundary = boundary;
                    }),
                    expected,
                    "{role:?} with {boundary:?}"
                );
            }
        }
    }

    #[test]
    fn refusal_messages_name_the_field_the_value_and_the_bound() {
        let cases = [
            (Refusal::EmptyField("program"), "program must not be empty"),
            (Refusal::EmbeddedNul("args"), "args must not contain NUL"),
            (
                Refusal::InvalidSchemaVersion("2".to_string()),
                "unsupported command schema version \"2\"",
            ),
            (
                Refusal::InvalidRelativePath {
                    field: "cwd",
                    value: "../outside".to_string(),
                },
                "cwd must be a root-relative path: \"../outside\"",
            ),
            (
                Refusal::DisplayTooLong,
                "display must be at most 512 characters",
            ),
            (
                Refusal::InvalidTimeout(0),
                "timeout must be between 1 and 3600000 ms, got 0",
            ),
            (
                Refusal::EmptyExpectedExitCodes,
                "expected_exit_codes must not be empty",
            ),
            (Refusal::EmptyPlatforms, "platforms must not be empty"),
            (
                Refusal::InvalidEnvironmentName("A=B".to_string()),
                "invalid environment variable name \"A=B\"",
            ),
            (
                Refusal::AuthorityRoleMismatch,
                "authority boundary does not match command role",
            ),
        ];
        for (refusal, message) in cases {
            assert_eq!(refusal.to_string(), message);
        }
    }

    #[test]
    fn broad_snapshot_recognition_preserves_permitted_command_word_spellings() {
        // Literal policy cases, not a second tokenizer or general cost oracle.
        for prefix in [
            "ripr  agent verify",
            "ripr\tagent verify",
            "ripr agent\tverify",
            "ripr \t agent \t verify",
            "r'i'pr agent verify",
            "\"ripr\" agent verify",
            "ripr 'agent' verify",
            "ripr ag\"en\"t verify",
            "ripr agent \"verify\"",
            "ripr agent v\"er\"ify",
        ] {
            let command = format!(
                "{prefix} --root . --before target/ripr/reports/before.repo-exposure.json --after target/ripr/reports/after.repo-exposure.json --json"
            );
            assert!(
                verify_command_is_unbounded_repo_exposure_snapshot_compare(&command),
                "the shared owner must retain the known broad-snapshot exclusion: {command:?}"
            );
        }
        for bounded in [
            "cargo test -p ordinary_shop above_threshold_discounts",
            "cargo  test\t-p ordinary_shop above_threshold_discounts",
            "ripr agent verify --root . --json",
        ] {
            assert!(
                !verify_command_is_unbounded_repo_exposure_snapshot_compare(bounded),
                "recognition must remain limited to the known before/after snapshot command: {bounded:?}"
            );
        }
    }
}
