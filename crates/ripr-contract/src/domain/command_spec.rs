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
    python_pytest_verify_display_for("python", path, node, windows)
}

/// [`python_pytest_verify_display`] spelled with one of
/// [`crate::domain::PYTEST_VERIFY_INTERPRETERS`] (#13801). Any other
/// interpreter yields no display.
pub fn python_pytest_verify_display_for(
    interpreter: &str,
    path: &str,
    node: &str,
    windows: bool,
) -> Option<String> {
    if !crate::domain::PYTEST_VERIFY_INTERPRETERS.contains(&interpreter) {
        return None;
    }
    let selector = if windows {
        shell_quote_file_arg_for_host(&format!("{path}::{node}"), true)?
    } else {
        format!("{}::{node}", shell_quote_file_arg_for_host(path, false)?)
    };
    Some(format!("{interpreter} -m pytest {selector}"))
}

/// The Python producer's `unittest` display for `qualified` in the file at
/// `path`. Returns `None` when the path holds a literal dot, since `unittest`
/// would read it as a module separator and could select a different file.
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

/// A typed description of one command route a producer offers to a consumer. It
/// describes the route; it does not authorize running it, and `display` is
/// never parsed back into `program` and `args`.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CommandSpec {
    /// Must equal [`CommandSpec::SCHEMA_VERSION`].
    pub schema_version: String,
    /// The route identity; must be non-empty.
    pub command_id: String,
    /// What the route is for; must match `authority_boundary`.
    pub role: CommandRole,
    /// How the route may be run.
    pub execution_mode: CommandExecutionMode,
    /// The program to run; must be non-empty.
    pub program: String,
    /// The ordered arguments, passed as argv without shell parsing.
    pub args: Vec<String>,
    /// The working directory, relative to the selected root. Serialized as
    /// `working_directory`.
    #[serde(rename = "working_directory", alias = "cwd")]
    pub cwd: String,
    /// Environment variables the route sets.
    pub env_set: Vec<EnvironmentAssignment>,
    /// Environment variable names the route passes through from the caller.
    pub env_passthrough: Vec<String>,
    /// How the route's environment is built. Serialized as `environment`.
    #[serde(rename = "environment", alias = "environment_policy")]
    pub environment_policy: EnvironmentPolicy,
    /// What the route reads on stdin.
    pub stdin: StdinPolicy,
    /// The timeout in milliseconds, from 1 to 3,600,000.
    pub timeout_ms: u64,
    /// Whether the route may be cancelled.
    pub cancellation: CancellationPolicy,
    /// What network access the route needs. Serialized as `network`.
    #[serde(rename = "network", alias = "network_policy")]
    pub network_policy: NetworkPolicy,
    /// How the route's result is read.
    pub expected_result_parser: ExpectedResultParser,
    /// Exit codes that count as the expected result; must be non-empty.
    pub expected_exit_codes: Vec<i32>,
    /// Root-relative paths the route writes.
    pub expected_writes: Vec<String>,
    /// The route's cost class.
    pub cost_class: CommandCostClass,
    /// Platforms the route supports; must be non-empty.
    pub platforms: Vec<CommandPlatform>,
    /// The human display string, at most 512 characters. Never execution
    /// authority. Serialized as `human_display`.
    #[serde(rename = "human_display", alias = "display")]
    pub display: String,
    /// The authority the route is limited to; must match `role`.
    pub authority_boundary: CommandAuthorityBoundary,
}

/// What a command route is for.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandRole {
    /// `verify`: check a repair.
    Verify,
    /// `receipt`: write a receipt.
    Receipt,
    /// `regeneration`: regenerate an artifact.
    Regeneration,
    /// `inspection`: inspect state without changing it.
    Inspection,
    /// `targeted_rerun`: rerun a narrowed selection.
    TargetedRerun,
    /// Start one repair attempt for a single bound seam (#6305). The
    /// task-first repair command offers this route only when its producer
    /// binds exactly one eligible subject.
    RepairStart,
}

/// How a command route may be run.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandExecutionMode {
    /// `direct`: run `program` with `args` as argv, with no shell.
    Direct,
    /// `shell_required`: the route needs a shell, for example for output
    /// redirection.
    ShellRequired,
    /// `manual`: a person runs the route.
    Manual,
}

/// What a route reads on stdin.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StdinPolicy {
    /// `null`: stdin is empty.
    Null,
    /// `declared`: the route declares its stdin.
    Declared,
}

/// Whether a running route may be cancelled.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationPolicy {
    /// `allowed`.
    Allowed,
    /// `not_allowed`.
    NotAllowed,
}

/// What network access a route needs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    /// `forbidden`: the route must not use the network.
    Forbidden,
    /// `not_required`: the route does not need the network.
    NotRequired,
    /// `declared`: the route declares the network access it needs.
    Declared,
    /// `unrestricted`: the route may use any network access.
    Unrestricted,
}

/// How a route's environment is built.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentPolicy {
    /// `clean`: start from an empty environment.
    Clean,
    /// `declared`: use only the declared variables.
    Declared,
    /// `inherited`: inherit the caller's environment.
    Inherited,
}

/// One environment variable a route sets.
#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EnvironmentAssignment {
    /// The variable name; non-empty, with no `=`, NUL or whitespace.
    pub name: String,
    /// The value; must not contain NUL.
    pub value: String,
}

/// How a route's result is read.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedResultParser {
    /// `exit_code`: from the exit code.
    ExitCode,
    /// `declared_json`: from the declared JSON output.
    DeclaredJson,
    /// `declared_text`: from the declared text output.
    DeclaredText,
}

/// The cost class of a command route, for scheduling.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandCostClass {
    /// `unknown`: the producer does not know the cost.
    Unknown,
    /// `pilot`.
    Pilot,
    /// `check`.
    Check,
    /// `projection_only`: renders existing evidence.
    ProjectionOnly,
    /// `bounded_static_analysis`.
    BoundedStaticAnalysis,
    /// `full_repo_analysis`.
    FullRepoAnalysis,
    /// `compile_or_test`: compiles or runs tests.
    CompileOrTest,
    /// `runtime_mutation_or_import`.
    RuntimeMutationOrImport,
}

/// A platform a route supports.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandPlatform {
    /// `linux`.
    Linux,
    /// `macos`.
    Macos,
    /// `windows`.
    Windows,
}

/// The authority a route is limited to. Each role has exactly one matching
/// boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandAuthorityBoundary {
    /// `verification_route_only`, for `verify`.
    VerificationRouteOnly,
    /// `receipt_route_only`, for `receipt`.
    ReceiptRouteOnly,
    /// `regeneration_route_only`, for `regeneration`.
    RegenerationRouteOnly,
    /// `inspection_route_only`, for `inspection`.
    InspectionRouteOnly,
    /// `targeted_rerun_route_only`, for `targeted_rerun`.
    TargetedRerunRouteOnly,
    /// `repair_start_route_only`, for `repair_start`.
    RepairStartRouteOnly,
}

/// Why [`CommandSpec::validate`] refused a spec.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommandSpecValidationError {
    /// A required field is empty or blank.
    EmptyField(&'static str),
    /// A field contains a NUL character.
    EmbeddedNul(&'static str),
    /// The schema version is not [`CommandSpec::SCHEMA_VERSION`].
    InvalidSchemaVersion(String),
    /// A path is not root-relative: it is absolute, has a drive prefix or
    /// climbs with `..`. For `cwd`, a scheme prefix or a line terminator is
    /// also refused.
    InvalidRelativePath {
        /// The field holding the path.
        field: &'static str,
        /// The value found.
        value: String,
    },
    /// `display` is longer than 512 characters.
    DisplayTooLong,
    /// The timeout is zero or above 3,600,000 ms.
    InvalidTimeout(u64),
    /// `expected_exit_codes` is empty.
    EmptyExpectedExitCodes,
    /// `platforms` is empty.
    EmptyPlatforms,
    /// An environment variable name is empty or holds `=`, NUL or whitespace.
    InvalidEnvironmentName(String),
    /// `authority_boundary` does not match `role`.
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
    /// The only supported schema version.
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
        validate_working_directory(&self.cwd)?;
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

fn validate_working_directory(value: &str) -> Result<(), CommandSpecValidationError> {
    validate_root_relative_path("cwd", value)?;
    // The published working_directory pattern excludes ECMAScript line
    // terminators and a nonempty scheme prefix before the first separator.
    let has_scheme_prefix = value
        .split(['/', '\\'])
        .next()
        .is_some_and(|first| first.chars().skip(1).any(|ch| ch == ':'));
    if has_scheme_prefix || value.contains(['\r', '\n', '\u{2028}', '\u{2029}']) {
        return Err(CommandSpecValidationError::InvalidRelativePath {
            field: "cwd",
            value: value.to_owned(),
        });
    }
    Ok(())
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
        // Drive-letter paths are assembled here because the local-context
        // gate refuses a literal absolute Windows path in source.
        let upper = char::from(b'C');
        let lower = char::from(b'c');
        let escapes = [
            "..".to_string(),
            "../outside".to_string(),
            "crates/../../outside".to_string(),
            "crates\\..\\outside".to_string(),
            "/etc".to_string(),
            "\\\\server\\share".to_string(),
            "\\outside".to_string(),
            format!("{upper}:/outside"),
            format!("{upper}:outside"),
            format!("{lower}:\\outside"),
        ];
        for escape in &escapes {
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
    fn working_directory_matches_published_line_and_scheme_rules() -> Result<(), String> {
        for cwd in [
            "a\n",
            "a\rb",
            "a\u{2028}b",
            "a\u{2029}b",
            "src:gen/x",
            ":x:y",
        ] {
            let expected = Err(Refusal::InvalidRelativePath {
                field: "cwd",
                value: cwd.into(),
            });
            if validated(|spec| spec.cwd = cwd.into()) != expected {
                return Err(format!("unexpected working_directory verdict for {cwd:?}"));
            }
        }
        for cwd in [":literal", "src/gen:x", "src\\gen:x"] {
            validated(|spec| spec.cwd = cwd.into()).map_err(|err| err.to_string())?;
        }
        validated(|spec| spec.expected_writes = vec!["src:gen/x".into()])
            .map_err(|err| err.to_string())
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

    #[test]
    fn pytest_display_quotes_for_the_named_host_only() {
        let display = |path: &str, windows: bool| {
            python_pytest_verify_display(path, "TestA::test_b", windows)
        };
        // A plain path stays readable on both hosts; on Windows the `::`
        // separator joins the token and must not force quotes.
        for windows in [false, true] {
            assert_eq!(
                display("tests/test_a.py", windows).as_deref(),
                Some("python -m pytest tests/test_a.py::TestA::test_b"),
                "windows={windows}"
            );
        }
        assert_eq!(
            display("my tests/test_a.py", false).as_deref(),
            Some("python -m pytest 'my tests/test_a.py'::TestA::test_b")
        );
        assert_eq!(
            display("my tests/test_a.py", true).as_deref(),
            Some("python -m pytest \"my tests/test_a.py::TestA::test_b\"")
        );
        // A colon is an ordinary byte on Windows and quoted on POSIX.
        assert_eq!(
            display("a:b.py", false).as_deref(),
            Some("python -m pytest 'a:b.py'::TestA::test_b")
        );
        assert_eq!(
            display("it's.py", false).as_deref(),
            Some("python -m pytest 'it'\\''s.py'::TestA::test_b")
        );
        assert_eq!(
            display("say \"hi\".py", true).as_deref(),
            Some("python -m pytest \"say \"\"hi\"\".py::TestA::test_b\"")
        );
        // An empty token is quoted, never emitted bare.
        assert_eq!(
            display("", false).as_deref(),
            Some("python -m pytest ''::TestA::test_b")
        );
        // Bytes that Windows shells expand inside double quotes get no command.
        for path in ["$(x).py", "`x`.py", "%X%.py"] {
            assert_eq!(display(path, true), None, "{path:?}");
            assert!(display(path, false).is_some(), "{path:?}");
        }
    }

    #[test]
    fn unittest_display_quotes_the_module_for_the_named_host_only() {
        let display = |path: &str, windows: bool| {
            python_unittest_verify_display(path, "TestA.test_b", windows)
        };
        for windows in [false, true] {
            assert_eq!(
                display("tests/test_a.py", windows).as_deref(),
                Some("python -m unittest tests.test_a.TestA.test_b"),
                "windows={windows}"
            );
        }
        assert_eq!(
            display("my tests/test_a.py", false).as_deref(),
            Some("python -m unittest 'my tests.test_a'.TestA.test_b")
        );
        assert_eq!(
            display("my tests/test_a.py", true).as_deref(),
            Some("python -m unittest \"my tests.test_a.TestA.test_b\"")
        );
        // A literal dot in the path would select a different module.
        assert_eq!(display("tests/v1.2/test_a.py", false), None);
    }

    #[test]
    fn command_tokens_drop_empty_words_and_keep_quoted_whitespace() {
        assert_eq!(
            command_tokens("a  'b c'\t\"d e\"  f"),
            ["a", "b c", "d e", "f"]
        );
        assert_eq!(command_tokens("   "), Vec::<String>::new());
    }

    #[test]
    fn broad_snapshot_recognition_needs_every_marker() {
        for partial in [
            "ripr agent verify --before target/before.repo-exposure.json --json",
            "ripr agent verify --before before.json --after after.json",
            "ripr agent verify --after target/after.repo-exposure.json",
            "cat --before x --after target/after.repo-exposure.json",
            "cat target/after.repo-exposure.json",
        ] {
            assert!(
                !verify_command_is_unbounded_repo_exposure_snapshot_compare(partial),
                "{partial:?}"
            );
        }
    }

    #[test]
    fn pytest_display_admits_only_the_listed_interpreters() {
        for interpreter in crate::domain::PYTEST_VERIFY_INTERPRETERS {
            assert_eq!(
                python_pytest_verify_display_for(interpreter, "t.py", "test_a", false),
                Some(format!("{interpreter} -m pytest t.py::test_a"))
            );
        }
        for refused in ["python3", "", "/usr/bin/python", "env/bin/python"] {
            assert_eq!(
                python_pytest_verify_display_for(refused, "t.py", "test_a", false),
                None,
                "{refused:?}"
            );
        }
    }

    #[test]
    fn unittest_module_accepts_either_separator_and_an_extensionless_path() {
        for path in ["tests\\test_cart.py", "tests/test_cart"] {
            assert_eq!(
                python_unittest_verify_display(path, "TestCart.test_total", false),
                Some("python -m unittest tests.test_cart.TestCart.test_total".to_string()),
                "{path}"
            );
        }
    }

    #[test]
    fn only_the_two_spec_sources_are_actionable_verify_provenance() {
        assert!(actionable_verify_source_is_supported(
            "canonical_item.verify_command"
        ));
        assert!(actionable_verify_source_is_supported(
            "related_test_or_observer.name"
        ));
        for other in [
            "",
            "canonical_item",
            "related_test_or_observer",
            "verify_command",
        ] {
            assert!(!actionable_verify_source_is_supported(other), "{other:?}");
        }
    }
}
