//! A direct record literal supplies one field to the canonical arm selector.
//! Local helper results and field writes delegate to the existing value/binding
//! authority; coercions and computed inputs remain unresolved.

use super::is_identifier;
#[cfg(feature = "lang-rust")]
use super::literal_value;

#[derive(Clone, Debug)]
pub(super) struct FieldSelector {
    field: String,
    record_type: String,
    owner_body: String,
    owner_name: String,
    owner_start_line: usize,
    original_pattern_admitted: bool,
    admitted: bool,
    source: Option<String>,
}

impl FieldSelector {
    pub(super) fn new(
        field: &str,
        record_type: &str,
        owner_body: &str,
        owner_name: &str,
        owner_start_line: usize,
    ) -> Option<Self> {
        (is_identifier(field) && is_identifier(record_type)).then(|| Self {
            field: field.to_string(),
            record_type: record_type.to_string(),
            owner_body: owner_body.to_string(),
            owner_name: owner_name.to_string(),
            owner_start_line,
            original_pattern_admitted: true,
            admitted: false,
            source: None,
        })
    }

    /// The canonical matcher intentionally has a wider historical pattern
    /// grammar. This field slice accepts only prelude Option constructors;
    /// a qualified constant named `None` is not that unit variant.
    fn supported_pattern(pattern: &str) -> bool {
        super::top_level_alternatives(pattern).is_some_and(|alternatives| {
            alternatives.into_iter().all(|pattern| {
                pattern == "None"
                    || pattern == "_"
                    || super::variant_parts(pattern).is_some_and(|parts| {
                        parts.qualifier.is_none()
                            && parts.name == "Some"
                            && parts.payload.is_some_and(super::payload_is_irrefutable)
                    })
            })
        })
    }

    pub(super) fn require_original_pattern(&mut self, pattern: &str) {
        self.original_pattern_admitted &= Self::supported_pattern(pattern);
    }

    /// The record and standard Option constructors must have one unambiguous
    /// meaning in the same source file as every related test. This bounded
    /// shape deliberately does not resolve imported records or aliases.
    #[cfg(feature = "lang-rust")]
    pub(super) fn admit_source(&mut self, source: &str) {
        use ra_ap_syntax::{
            AstNode,
            ast::{self, HasAttrs, HasGenericParams, HasName},
        };
        self.admitted = false;
        self.source = None;
        let Some(parse) = crate::analysis::syntax::parse_clean_source_file(source) else {
            return;
        };
        let tree = parse.tree();
        // Reuse the existing value-binding authority, including raw names,
        // prelude-pattern distinctions and opaque glob refusal (#7712).
        if ["Some", "None"]
            .into_iter()
            .any(|name| crate::analysis::syntax::source_binds_value_name(source, name))
        {
            return;
        }
        if !self.original_pattern_admitted {
            return;
        }
        // Bind this source declaration to the owner the selector established,
        // rather than retaining admission across a changed owner body.
        let functions = tree
            .syntax()
            .children()
            .filter_map(ast::Fn::cast)
            .filter(|function| {
                function
                    .name()
                    .is_some_and(|name| name.text() == self.owner_name)
            })
            .collect::<Vec<_>>();
        let [function] = functions.as_slice() else {
            return;
        };
        // Facts begin at the fn token, while the AST item includes preceding
        // doc comments. Match the same source span without erasing qualifiers,
        // executable attributes or a different declaration's identity.
        if function.async_token().is_some()
            || function.unsafe_token().is_some()
            || function.const_token().is_some()
            || function.abi().is_some()
            || function.generic_param_list().is_some()
            || function.attrs().next().is_some()
        {
            return;
        }
        let Some(token) = function.fn_token() else {
            return;
        };
        let start = usize::from(token.text_range().start());
        let end = usize::from(function.syntax().text_range().end());
        if source[..start]
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            + 1
            != self.owner_start_line
            || source.get(start..end) != Some(self.owner_body.as_str())
        {
            return;
        }
        let Some(ast::Expr::MatchExpr(matched)) = function.body().and_then(|body| body.tail_expr())
        else {
            return;
        };
        let Some(arms) = matched.match_arm_list() else {
            return;
        };
        if arms.arms().any(|arm| {
            arm.pat().is_none_or(|pattern| {
                !Self::supported_pattern(&pattern.syntax().text().to_string())
            })
        }) {
            return;
        }
        // A declaration or import can replace a prelude constructor/type. A
        // module-level macro may introduce an unread declaration too.
        if tree.syntax().descendants().any(|node| {
            ast::Name::cast(node.clone()).is_some_and(|name| {
                (name.text().strip_prefix("r#").unwrap_or(name.text()) == self.record_type
                    && !name
                        .syntax()
                        .parent()
                        .is_some_and(|parent| ast::Struct::can_cast(parent.kind())))
                    || matches!(
                        name.text().strip_prefix("r#").unwrap_or(name.text()),
                        "Option"
                            | "std"
                            | "core"
                            | "u64"
                            | "u128"
                            | "Debug"
                            | "Clone"
                            | "Copy"
                            | "PartialEq"
                            | "Eq"
                    )
            }) || ast::NameRef::cast(node.clone()).is_some_and(|name| {
                name.text().strip_prefix("r#").is_some_and(|plain| {
                    plain == self.record_type
                        || matches!(
                            plain,
                            "Option"
                                | "std"
                                | "core"
                                | "u64"
                                | "u128"
                                | "Debug"
                                | "Clone"
                                | "Copy"
                                | "PartialEq"
                                | "Eq"
                        )
                })
            }) || ast::MacroRules::can_cast(node.kind())
                || ast::MacroCall::cast(node.clone()).is_some_and(|call| {
                    if !node.ancestors().any(|p| ast::Fn::can_cast(p.kind())) {
                        return true;
                    }
                    // An opaque statement expansion can introduce caller-visible
                    // bindings such as `const None = Some(1001)`. Expression
                    // expansions cannot introduce bindings into the caller scope.
                    if !crate::analysis::syntax::macro_call_is_statement(&call) {
                        return false;
                    }
                    let path = call.path().map(|path| path.syntax().text().to_string());
                    path.as_deref().is_none_or(|name| {
                        !crate::analysis::syntax::TRUSTED_MACRO_NAMES.contains(&name)
                            || crate::analysis::syntax::source_binds_value_name(source, name)
                    })
                })
        }) {
            return;
        }
        // Unknown attributes can expand unread bindings. Admit only the
        // built-in shapes used by this slice; imported derive names above
        // are refused just like shadowed constructors.
        if tree
            .syntax()
            .descendants()
            .filter_map(ast::Attr::cast)
            .any(|attr| {
                let text = attr
                    .syntax()
                    .text()
                    .to_string()
                    .chars()
                    .filter(|ch| !ch.is_whitespace())
                    .collect::<String>();
                if let Some(names) = text
                    .strip_prefix("#[derive(")
                    .and_then(|s| s.strip_suffix(")]"))
                {
                    return !names
                        .split(',')
                        .filter(|name| !name.is_empty())
                        .all(|name| {
                            matches!(name, "Debug" | "Clone" | "Copy" | "PartialEq" | "Eq")
                        });
                }
                !(text == "#[test]"
                    || text.starts_with("#[cfg(")
                    || text.starts_with("#![cfg(")
                    || ["allow", "warn", "deny", "forbid"].iter().any(|lint| {
                        text.starts_with(&format!("#[{lint}("))
                            || text.starts_with(&format!("#![{lint}("))
                    }))
            })
        {
            return;
        }
        if super::flattened_use_paths(source).iter().any(|import| {
            let leaf = import.path.rsplit("::").next().unwrap_or_default();
            let bound = import.alias.as_deref().unwrap_or(leaf);
            let bound = bound.strip_prefix("r#").unwrap_or(bound);
            matches!(
                bound,
                "Option"
                    | "std"
                    | "core"
                    | "u64"
                    | "u128"
                    | "Debug"
                    | "Clone"
                    | "Copy"
                    | "PartialEq"
                    | "Eq"
            ) || bound == self.record_type
                || (leaf == "*" && import.path != "super::*")
        }) {
            return;
        }
        // A root-level super glob imports an unread file; an inline test
        // module's super glob imports this already checked file.
        if tree
            .syntax()
            .descendants()
            .any(|node| super::parent_glob_outside_this_file(&node))
        {
            return;
        }
        let records = tree
            .syntax()
            .descendants()
            .filter_map(ast::Struct::cast)
            .filter(|record| {
                record
                    .name()
                    .is_some_and(|name| name.text() == self.record_type)
            })
            .collect::<Vec<_>>();
        let [record] = records.as_slice() else { return };
        if record.syntax().parent().as_ref() != Some(tree.syntax())
            || record.generic_param_list().is_some()
            || record
                .attrs()
                .any(|attr| !attr.syntax().text().to_string().starts_with("#[derive("))
        {
            return;
        }
        let Some(ast::FieldList::RecordFieldList(fields)) = record.field_list() else {
            return;
        };
        let selected = fields
            .fields()
            .filter(|field| field.name().is_some_and(|name| name.text() == self.field))
            .collect::<Vec<_>>();
        let [selected] = selected.as_slice() else {
            return;
        };
        if selected.attrs().next().is_some() {
            return;
        }
        let Some(ty) = selected.ty() else { return };
        let ty = ty
            .syntax()
            .text()
            .to_string()
            .chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>();
        self.admitted = matches!(ty.as_str(), "Option<u64>" | "Option<u128>");
        if self.admitted {
            self.source = Some(source.to_string());
        }
    }

    #[cfg(not(feature = "lang-rust"))]
    pub(super) fn admit_source(&mut self, _source: &str) {}

    pub(super) fn source_is_admitted(&self) -> bool {
        self.admitted
    }

    #[cfg(feature = "lang-rust")]
    pub(super) fn project_at_call(
        &self,
        input: &str,
        test: &crate::analysis::rust_index::TestSummary,
        call_line: usize,
        call_text: &str,
    ) -> Option<String> {
        if let Some(value) = self.project(input) {
            return Some(value.to_string());
        }
        let source = self.source.as_deref()?;
        let facts = crate::analysis::value_resolution::ValueEnvFacts::build(
            test,
            &crate::analysis::value_resolution::FileValueScan::default(),
        );
        facts.option_field_at_call(
            test,
            source,
            input,
            &self.record_type,
            &self.field,
            (call_line, call_text),
        )
    }

    #[cfg(not(feature = "lang-rust"))]
    pub(super) fn project_at_call(
        &self,
        _input: &str,
        _test: &crate::analysis::rust_index::TestSummary,
        _call_line: usize,
        _call_text: &str,
    ) -> Option<String> {
        None
    }

    /// Return a slice of this exact call's field initializer, preserving its
    /// constructor. Other fields and expected/diagnostic values cannot supply
    /// the selector.
    #[cfg(feature = "lang-rust")]
    pub(super) fn project<'a>(&self, input: &'a str) -> Option<&'a str> {
        use ra_ap_syntax::{
            AstNode,
            ast::{self, HasAttrs},
        };
        if !self.admitted {
            return None;
        }
        let prefix = "fn f() { let _ = ";
        let source = format!("{prefix}{input}; }}");
        let parse = crate::analysis::syntax::parse_clean_source_file(&source)?;
        let tree = parse.tree();
        let binding = tree.syntax().descendants().find_map(ast::LetStmt::cast)?;
        let ast::Expr::RefExpr(borrow) = binding.initializer()? else {
            return None;
        };
        if borrow.mut_token().is_some() {
            return None;
        }
        let ast::Expr::RecordExpr(record) = borrow.expr()? else {
            return None;
        };
        if record.path()?.syntax().text().to_string() != self.record_type {
            return None;
        }
        let fields = record.record_expr_field_list()?;
        if fields.dotdot_token().is_some() {
            return None;
        }
        let mut selected = None;
        let mut seen = std::collections::BTreeSet::new();
        for field in fields.fields() {
            let name = field.name_ref()?.text().to_string();
            if !seen.insert(name.clone())
                || field.colon_token().is_none()
                || field.attrs().next().is_some()
            {
                return None;
            }
            let expression = field.expr()?;
            let range = expression.syntax().text_range();
            let start = usize::from(range.start()).checked_sub(prefix.len())?;
            let end = usize::from(range.end()).checked_sub(prefix.len())?;
            let value = input.get(start..end)?;
            let closed = value == "None"
                || value
                    .strip_prefix("Some(")
                    .and_then(|v| v.strip_suffix(')'))
                    .is_some_and(|payload| literal_value(payload).is_some())
                || literal_value(value).is_some();
            if !closed {
                return None;
            }
            if name == self.field {
                if !(value == "None" || value.strip_prefix("Some(").and_then(|v| v.strip_suffix(')'))
                    .is_some_and(|payload| matches!(literal_value(payload), Some(super::LiteralValue::Int(value)) if value >= 0))) {
                    return None;
                }
                selected = Some(value);
            }
        }
        selected
    }

    #[cfg(not(feature = "lang-rust"))]
    pub(super) fn project<'a>(&self, _input: &'a str) -> Option<&'a str> {
        None
    }
}

#[cfg(all(test, feature = "lang-rust"))]
mod tests {
    use super::*;
    const OWNER: &str = "fn reason(lease: &Lease) -> u64 { match lease.elapsed { Some(ms) => ms as u64, None => lease.timeout } }";
    const RECORD: &str = "struct Lease { elapsed: Option<u128>, timeout: u64 }\nfn reason(lease: &Lease) -> u64 { match lease.elapsed { Some(ms) => ms as u64, None => lease.timeout } }";

    #[test]
    fn direct_record_projection_keeps_the_exact_field_and_constructor() -> Result<(), String> {
        let mut field =
            FieldSelector::new("elapsed", "Lease", OWNER, "reason", 2).ok_or("field descriptor")?;
        assert_eq!(
            field.project("&Lease { elapsed: None, timeout: 600 }"),
            None
        );
        field.admit_source(RECORD);
        assert_eq!(
            field.project("&Lease { elapsed: Some(1001), timeout: 600 }"),
            Some("Some(1001)")
        );
        assert_eq!(
            field.project("&Lease { timeout: 1001, elapsed: None }"),
            Some("None")
        );
        for input in [
            "&Other { elapsed: Some(1001), timeout: 600 }",
            "&mut Lease { elapsed: Some(1001), timeout: 600 }",
            "&lease",
            "&fixture()",
            "&Lease { elapsed: compute(), timeout: 600 }",
            "&Lease { elapsed: Some(value), timeout: 600 }",
            "&Lease { elapsed: Some(1001), timeout: compute() }",
            "&Lease { elapsed: Some(1001), ..other }",
            "&Lease { elapsed: None, elapsed: Some(1001), timeout: 600 }",
            "&Lease { elapsed, timeout: 600 }",
            "&Lease { elapsed: 1001, timeout: 600 }",
            "&Lease { elapsed: Some(false), timeout: 600 }",
            "&Lease { elapsed: Some(-1), timeout: 600 }",
            "&Lease { #[cfg(test)] elapsed: Some(1001), timeout: 600 }",
        ] {
            assert_eq!(field.project(input), None, "{input}");
        }
        Ok(())
    }

    #[test]
    fn unread_statement_macros_refuse_field_identity_but_standard_assertions_remain()
    -> Result<(), String> {
        let input = "&Lease { elapsed: None, timeout: 600 }";
        for (body, admitted) in [
            (
                "fn test() { assert_eq!(reason(&Lease { elapsed: None, timeout: 600 }), 600); }",
                true,
            ),
            ("fn test() { foreign::shadow!(); }", false),
            ("fn test() { foreign::shadow! {} }", false),
            (
                "fn test() { use foreign::shadow as assert_eq; assert_eq!(); }",
                false,
            ),
            ("fn test() { foreign::assert_eq!(); }", false),
        ] {
            let mut field = FieldSelector::new("elapsed", "Lease", OWNER, "reason", 2)
                .ok_or("field descriptor")?;
            field.admit_source(&format!("{RECORD}\n{body}"));
            assert_eq!(field.project(input).is_some(), admitted, "{body}");
        }
        Ok(())
    }

    #[test]
    fn record_and_option_namespace_ambiguity_never_admits_a_field() -> Result<(), String> {
        for source in [
            format!("{RECORD}\nfn Some(_: u128) -> Option<u128> {{ None }}"),
            format!("{RECORD}\nmod tests {{ type Lease = Other; }}"),
            format!("{RECORD}\nuse other::Option;"),
            format!("{RECORD}\nuse other::*;"),
            format!("{RECORD}\nuse super::*;"),
            format!("{RECORD}\nmacro_rules! generated {{ () => {{}} }}"),
            RECORD.replace("Option<u128>", "Other<u128>"),
            RECORD.replace("struct Lease", "struct Lease<T>"),
            RECORD.replace("elapsed: Option", "#[cfg(hidden)] elapsed: Option"),
            format!("mod nested {{ {RECORD} }}"),
            format!("#[derive(Custom)] {RECORD}"),
            format!("{RECORD}\nuse other::Debug;"),
            format!("{RECORD}\n#[custom] fn generated() {{}}"),
            format!("{RECORD}\nconst r#None: Option<u128> = Some(1001);"),
            format!("{RECORD}\nmod tests {{ type r#Lease = Other; }}"),
            format!("{RECORD}\nuse other::r#None;"),
            format!("{RECORD}\nuse other::Record as r#Lease;"),
            RECORD.replace("None => lease.timeout", "foreign::None => lease.timeout"),
        ] {
            let mut field = FieldSelector::new("elapsed", "Lease", OWNER, "reason", 2)
                .ok_or("field descriptor")?;
            field.admit_source(&source);
            assert_eq!(
                field.project("&Lease { elapsed: Some(1001), timeout: 600 }"),
                None,
                "{source}"
            );
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "lang-rust"))]
mod pattern_currentness_tests {
    use super::*;
    #[test]
    fn owner_doc_comments_preserve_only_the_exact_unconditional_root_declaration()
    -> Result<(), String> {
        let body = "fn reason(lease: &Lease) -> u64 { match lease.elapsed { Some(ms) => ms as u64, None => lease.timeout } }";
        let record = "struct Lease { elapsed: Option<u128>, timeout: u64 }";
        let input = "&Lease { elapsed: None, timeout: 600 }";
        let documented = format!("{record}\n/// Owner documentation.\n{body}");
        let mut field =
            FieldSelector::new("elapsed", "Lease", body, "reason", 3).ok_or("field descriptor")?;
        field.admit_source(&documented);
        assert_eq!(field.project(input), Some("None"));
        for visibility in ["pub", "pub(crate)"] {
            field.admit_source(
                &documented.replace("fn reason(", &format!("{visibility} fn reason(")),
            );
            assert_eq!(field.project(input), Some("None"), "{visibility}");
        }
        for prefix in [
            "#[cfg(any())]",
            "#[cfg_attr(all(), custom)]",
            "#[custom]",
            "#[doc = \"documented\"]",
            "async",
            "unsafe",
            "const",
            "extern \"C\"",
        ] {
            let source = format!("{record}\n/// Owner documentation.\n{prefix} {body}");
            field.admit_source(&source);
            assert_eq!(field.project(input), None, "{prefix}");
        }
        for source in [
            documented.replace("fn reason(", "fn reason<T>("),
            format!("{documented}\n{body}"),
            format!("{record}\nmod nested {{\n/// Owner documentation.\n{body}\n}}"),
            format!(
                "{record}\n/// Owner documentation.\n{}",
                body.replace("None => lease.timeout", "None => 1")
            ),
            format!(
                "{record}\n/// Owner documentation.\n#[cfg(any())] {body}\nuse foreign::reason;"
            ),
        ] {
            field.admit_source(&source);
            assert_eq!(field.project(input), None, "{source}");
        }
        let mut misplaced =
            FieldSelector::new("elapsed", "Lease", body, "reason", 2).ok_or("field descriptor")?;
        misplaced.admit_source(&documented);
        assert_eq!(
            misplaced.project(input),
            None,
            "a different source occurrence cannot lend its body"
        );
        Ok(())
    }

    #[test]
    fn original_foreign_patterns_and_changed_owner_bodies_cannot_borrow_admission()
    -> Result<(), String> {
        let body = "fn reason(lease: &Lease) -> u64 { match lease.elapsed { Some(ms) => ms as u64, None => lease.timeout } }";
        let source = format!("struct Lease {{ elapsed: Option<u128>, timeout: u64 }}\n{body}");
        let input = "&Lease { elapsed: None, timeout: 600 }";
        let mut field =
            FieldSelector::new("elapsed", "Lease", body, "reason", 2).ok_or("field descriptor")?;
        field.admit_source(&source);
        assert_eq!(field.project(input), Some("None"));
        field.admit_source(&source.replace("None => lease.timeout", "None => 1"));
        assert_eq!(field.project(input), None);
        field.admit_source(&source);
        assert_eq!(field.project(input), Some("None"));
        field.require_original_pattern("foreign::None");
        field.admit_source(&source);
        assert_eq!(field.project(input), None);
        Ok(())
    }
}
