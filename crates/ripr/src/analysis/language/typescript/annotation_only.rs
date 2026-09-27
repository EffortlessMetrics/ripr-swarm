//! Annotation-only signature change detection for the TypeScript preview
//! adapter (#4282).
//!
//! TypeScript erases type syntax, so a changed line whose old and new forms
//! differ only in type annotations (return type, parameter annotations,
//! optional `?` markers, generic parameter lists, a `this` parameter, a
//! variable annotation) has no runtime behavior and yields no probe. This is
//! the TypeScript counterpart of the Python adapter's
//! `is_annotation_only_def_change` (#1289).
//!
//! Each side is parsed with oxc as a one-line snippet and reduced to a
//! runtime skeleton: everything the emitted JavaScript keeps, and nothing
//! TypeScript erases. Equal skeletons mean an annotation-only change. The
//! check fails closed: a line that does not parse as a supported shape on
//! either side, or whose runtime text differs anywhere (a default value, a
//! parameter name, a body, an `as` cast inside an expression), keeps its
//! probe.

use super::*;

/// Whether a paired removed/added line change touches ONLY TypeScript type
/// syntax. Fails closed (returns `false`) on identical lines, on lines that do
/// not parse as a function/method signature or a variable declaration, and on
/// any runtime difference.
pub(crate) fn is_annotation_only_signature_change(
    file: &Path,
    old_line: &str,
    new_line: &str,
) -> bool {
    if old_line.trim() == new_line.trim() {
        return false;
    }
    match (
        runtime_skeleton(file, old_line),
        runtime_skeleton(file, new_line),
    ) {
        (Some(old), Some(new)) => old == new,
        _ => false,
    }
}

/// The runtime-significant shape of one changed line. Two lines with equal
/// skeletons emit the same JavaScript.
#[derive(Debug, PartialEq, Eq)]
enum RuntimeSkeleton {
    Function {
        /// `export` / `export default` / none: part of the runtime module shape.
        export: &'static str,
        function: FunctionSkeleton,
    },
    Method {
        decorators: Vec<String>,
        key: String,
        kind: String,
        computed: bool,
        is_static: bool,
        function: FunctionSkeleton,
    },
    Variables {
        export: &'static str,
        kind: String,
        declarators: Vec<(String, Option<InitSkeleton>)>,
    },
}

#[derive(Debug, PartialEq, Eq)]
enum InitSkeleton {
    Function(FunctionSkeleton),
    Arrow {
        is_async: bool,
        params: Vec<ParamSkeleton>,
        rest: Option<String>,
        body: String,
    },
    /// Any other initializer is compared by its exact source text, so type
    /// syntax inside an expression (`as`, `satisfies`, generic arguments)
    /// fails closed.
    Expression(String),
}

#[derive(Debug, PartialEq, Eq)]
struct FunctionSkeleton {
    name: Option<String>,
    is_async: bool,
    generator: bool,
    params: Vec<ParamSkeleton>,
    rest: Option<String>,
    /// `None` for an overload or `declare`d signature, which emits nothing.
    body: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
struct ParamSkeleton {
    decorators: Vec<String>,
    pattern: String,
    initializer: Option<String>,
    /// Parameter-property modifiers (`private x`, `readonly x`) emit a field
    /// assignment in the constructor, so they are runtime-significant.
    accessibility: Option<String>,
    readonly: bool,
    is_override: bool,
}

/// Snippet wrappers tried in order. A signature line that opens a body
/// (`function f(a: T): R {`) needs a closing brace to parse; a method
/// signature needs an enclosing class. The first wrapper that parses to one
/// supported statement wins; the skeleton records the shape it parsed as, so
/// a function line never matches a method line.
fn snippets(line: &str) -> [String; 4] {
    [
        line.to_string(),
        format!("{line}\n}}"),
        format!("class RiprAnnotationProbe {{\n{line}\n}}"),
        format!("class RiprAnnotationProbe {{\n{line}\n}}\n}}"),
    ]
}

fn runtime_skeleton(file: &Path, line: &str) -> Option<RuntimeSkeleton> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let candidates = snippets(trimmed);
    parse_on_worker(file, trimmed, move |file, _line, allocator| {
        candidates.iter().find_map(|snippet| {
            let ret = Parser::new(allocator, snippet, source_type_for(file)).parse();
            if !ret.errors.is_empty() || ret.program.body.len() != 1 {
                return None;
            }
            statement_skeleton(&ret.program.body[0], snippet)
        })
    })
    .ok()
    .flatten()
}

fn statement_skeleton(statement: &Statement<'_>, src: &str) -> Option<RuntimeSkeleton> {
    match statement {
        Statement::FunctionDeclaration(function) => Some(RuntimeSkeleton::Function {
            export: "",
            function: function_skeleton(function, src),
        }),
        Statement::VariableDeclaration(declaration) => variables_skeleton("", declaration, src),
        Statement::ExportNamedDeclaration(export) => match export.declaration.as_ref()? {
            Declaration::FunctionDeclaration(function) => Some(RuntimeSkeleton::Function {
                export: "export",
                function: function_skeleton(function, src),
            }),
            Declaration::VariableDeclaration(declaration) => {
                variables_skeleton("export", declaration, src)
            }
            _ => None,
        },
        Statement::ExportDefaultDeclaration(export) => match &export.declaration {
            ExportDefaultDeclarationKind::FunctionDeclaration(function) => {
                Some(RuntimeSkeleton::Function {
                    export: "export default",
                    function: function_skeleton(function, src),
                })
            }
            _ => None,
        },
        Statement::ClassDeclaration(class) => {
            // Only the synthetic wrapper class holding exactly one method.
            let [ClassElement::MethodDefinition(method)] = class.body.body.as_slice() else {
                return None;
            };
            Some(RuntimeSkeleton::Method {
                decorators: method
                    .decorators
                    .iter()
                    .map(|decorator| text(src, decorator.span))
                    .collect(),
                key: text(src, method.key.span()),
                kind: format!("{:?}", method.kind),
                computed: method.computed,
                is_static: method.r#static,
                function: function_skeleton(&method.value, src),
            })
        }
        _ => None,
    }
}

fn variables_skeleton(
    export: &'static str,
    declaration: &VariableDeclaration<'_>,
    src: &str,
) -> Option<RuntimeSkeleton> {
    if declaration.declare {
        return None;
    }
    let declarators = declaration
        .declarations
        .iter()
        .map(|declarator| {
            (
                text(src, declarator.id.span()),
                declarator
                    .init
                    .as_ref()
                    .map(|init| init_skeleton(init, src)),
            )
        })
        .collect();
    Some(RuntimeSkeleton::Variables {
        export,
        kind: format!("{:?}", declaration.kind),
        declarators,
    })
}

fn init_skeleton(init: &Expression<'_>, src: &str) -> InitSkeleton {
    match init {
        Expression::FunctionExpression(function) => {
            InitSkeleton::Function(function_skeleton(function, src))
        }
        Expression::ArrowFunctionExpression(arrow) => {
            let (params, rest) = params_skeleton(&arrow.params, src);
            InitSkeleton::Arrow {
                is_async: arrow.r#async,
                params,
                rest,
                body: text(src, arrow.body.span),
            }
        }
        other => InitSkeleton::Expression(text(src, other.span())),
    }
}

fn function_skeleton(function: &Function<'_>, src: &str) -> FunctionSkeleton {
    let (params, rest) = params_skeleton(&function.params, src);
    FunctionSkeleton {
        name: function.id.as_ref().map(|id| id.name.to_string()),
        is_async: function.r#async,
        generator: function.generator,
        params,
        rest,
        body: function.body.as_ref().map(|body| text(src, body.span)),
    }
}

fn params_skeleton(
    params: &FormalParameters<'_>,
    src: &str,
) -> (Vec<ParamSkeleton>, Option<String>) {
    let items = params
        .items
        .iter()
        .map(|param| ParamSkeleton {
            decorators: param
                .decorators
                .iter()
                .map(|decorator| text(src, decorator.span))
                .collect(),
            // The binding pattern's span excludes the parameter's own type
            // annotation and `?` marker, which live on `FormalParameter`.
            pattern: text(src, param.pattern.span()),
            initializer: param
                .initializer
                .as_ref()
                .map(|init| text(src, init.span())),
            accessibility: param
                .accessibility
                .map(|accessibility| format!("{accessibility:?}")),
            readonly: param.readonly,
            is_override: param.r#override,
        })
        .collect();
    let rest = params
        .rest
        .as_ref()
        .map(|rest| text(src, rest.rest.argument.span()));
    (items, rest)
}

fn text(src: &str, span: oxc_span::Span) -> String {
    src.get(span.start as usize..span.end as usize)
        .unwrap_or_default()
        .to_string()
}
