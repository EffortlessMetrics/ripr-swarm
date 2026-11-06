use super::*;

/// Admit the exported config's construction, rather than assuming the first
/// object literal in a merged call is the complete config (#9940). This is a
/// bounded source reader, not execution of arbitrary JavaScript.
pub(super) fn exported_config_is_unread(text: &str) -> bool {
    use oxc_ast_visit::{Visit, walk};

    fn export_member(object: &Expression<'_>, key: &str) -> bool {
        matches!(object.get_inner_expression(), Expression::Identifier(id)
            if (id.name == "module" && key == "exports") || (id.name == "exports" && key == "default"))
    }

    fn member_key<'e>(expression: &'e Expression<'_>) -> Option<&'e str> {
        match expression.get_inner_expression() {
            Expression::StringLiteral(key) => Some(key.value.as_str()),
            Expression::TemplateLiteral(template) if template.expressions.is_empty() => template
                .quasis
                .first()?
                .value
                .cooked
                .as_ref()
                .map(|key| key.as_str()),
            _ => None,
        }
    }

    fn commonjs(target: &oxc_ast::ast::AssignmentTarget<'_>) -> bool {
        match target {
            oxc_ast::ast::AssignmentTarget::StaticMemberExpression(member) => {
                export_member(&member.object, member.property.name.as_str())
            }
            oxc_ast::ast::AssignmentTarget::ComputedMemberExpression(member) => {
                member_key(&member.expression).is_some_and(|key| export_member(&member.object, key))
            }
            _ => false,
        }
    }

    fn override_key(key: &str) -> bool {
        matches!(
            key,
            "update"
                | "projects"
                | "workspace"
                | "root"
                | "snapshotResolver"
                | "resolveSnapshotPath"
                | "snapshotEnvironment"
                | "updateSnapshot"
                | "snapshotFormat"
        )
    }

    fn known_wrapper(program: &oxc_ast::ast::Program<'_>, name: &str) -> bool {
        program.body.iter().any(|statement| {
            let Statement::ImportDeclaration(import) = statement else { return false; };
            matches!(import.source.value.as_str(), "vitest/config" | "vite")
                && import.import_kind != oxc_ast::ast::ImportOrExportKind::Type
                && import.specifiers.iter().flatten().any(|specifier| {
                    matches!(specifier, oxc_ast::ast::ImportDeclarationSpecifier::ImportSpecifier(specifier)
                        if specifier.local.name == name && specifier.imported.name() == "defineConfig"
                        && specifier.import_kind != oxc_ast::ast::ImportOrExportKind::Type)
                })
        })
    }

    fn plain_config(
        expression: &Expression<'_>,
        program: &oxc_ast::ast::Program<'_>,
        bindings: &mut Vec<String>,
    ) -> bool {
        match expression.get_inner_expression() {
            Expression::ObjectExpression(object) => object.properties.iter().all(|property| {
                let oxc_ast::ast::ObjectPropertyKind::ObjectProperty(property) = property else {
                    return false;
                };
                let key = property.key.static_name();
                // Only the runner's `test` subsection is another config.
                // Transform maps and other nested data remain opaque values.
                (!property.computed || key.is_some())
                    && (key.as_deref() != Some("test")
                        || plain_config(&property.value, program, bindings))
            }),
            Expression::Identifier(id) => plain_binding(id.name.as_str(), program, bindings),
            Expression::CallExpression(call) => {
                matches!(call.callee.get_inner_expression(), Expression::Identifier(id) if known_wrapper(program, id.name.as_str()))
                    && call.arguments.len() == 1
                    && call.arguments[0]
                        .as_expression()
                        .is_some_and(|argument| plain_config(argument, program, bindings))
            }
            _ => false,
        }
    }

    fn plain_binding(
        name: &str,
        program: &oxc_ast::ast::Program<'_>,
        bindings: &mut Vec<String>,
    ) -> bool {
        if bindings.iter().any(|seen| seen == name) || bindings.len() >= 32 {
            return false;
        }
        bindings.push(name.to_owned());
        let mut declarations = program.body.iter().filter_map(|statement| match statement {
            Statement::VariableDeclaration(declaration) => Some(declaration.as_ref()),
            Statement::ExportNamedDeclaration(export) => match &export.declaration {
                Some(oxc_ast::ast::Declaration::VariableDeclaration(declaration)) => Some(declaration.as_ref()),
                _ => None,
            },
            _ => None,
        }).flat_map(|declaration| declaration.declarations.iter()).filter(|declarator| {
            matches!(&declarator.id, oxc_ast::ast::BindingPattern::BindingIdentifier(binding) if binding.name == name)
        });
        let Some(declaration) = declarations.next() else {
            return false;
        };
        declarations.next().is_none()
            && declaration
                .init
                .as_ref()
                .is_some_and(|initializer| plain_config(initializer, program, bindings))
    }

    fn config_receiver(expression: &Expression<'_>, bindings: &[String]) -> bool {
        match expression.get_inner_expression() {
            Expression::Identifier(id) => bindings.iter().any(|name| name == id.name.as_str()),
            Expression::StaticMemberExpression(member) => {
                export_member(&member.object, member.property.name.as_str())
                    || (member.property.name == "test" && config_receiver(&member.object, bindings))
            }
            Expression::ComputedMemberExpression(member) => member_key(&member.expression)
                .is_some_and(|key| {
                    export_member(&member.object, key)
                        || (key == "test" && config_receiver(&member.object, bindings))
                }),
            _ => false,
        }
    }

    // A later write must not introduce an alias whose earlier mutations
    // were outside the export's admitted identity set.
    fn plain_write(
        expression: &Expression<'_>,
        program: &oxc_ast::ast::Program<'_>,
        bindings: &[String],
    ) -> bool {
        let mut resolved = bindings.to_vec();
        plain_config(expression, program, &mut resolved) && resolved.len() == bindings.len()
    }

    struct Writes<'p, 'a> {
        program: &'p oxc_ast::ast::Program<'a>,
        bindings: Vec<String>,
        unread: bool,
    }
    impl<'a> Visit<'a> for Writes<'_, 'a> {
        fn visit_assignment_expression(
            &mut self,
            assignment: &oxc_ast::ast::AssignmentExpression<'a>,
        ) {
            use oxc_ast::ast::AssignmentTarget;
            if let AssignmentTarget::ComputedMemberExpression(member) = &assignment.left
                && config_receiver(&member.object, &self.bindings)
            {
                // Decode literal member names through the AST: escaped
                // `up\\u0064ate` must not evade the textual key checks.
                let unread_key = if let Some(key) = member_key(&member.expression) {
                    override_key(key)
                        || (key == "test"
                            && !plain_write(&assignment.right, self.program, &self.bindings))
                } else {
                    !matches!(
                        member.expression.get_inner_expression(),
                        Expression::NumericLiteral(_)
                            | Expression::BooleanLiteral(_)
                            | Expression::NullLiteral(_)
                    )
                };
                if unread_key {
                    self.unread = true;
                }
            }
            if let AssignmentTarget::StaticMemberExpression(member) = &assignment.left
                && member.property.name == "test"
                && config_receiver(&member.object, &self.bindings)
                && !plain_write(&assignment.right, self.program, &self.bindings)
            {
                self.unread = true;
            }
            // Replacing a resolved local config invalidates its initializer.
            if let AssignmentTarget::AssignmentTargetIdentifier(id) = &assignment.left
                && self.bindings.iter().any(|name| name == id.name.as_str())
            {
                self.unread = true;
            }
            walk::walk_assignment_expression(self, assignment);
        }
        fn visit_call_expression(&mut self, call: &oxc_ast::ast::CallExpression<'a>) {
            let wrapper = matches!(call.callee.get_inner_expression(), Expression::Identifier(id) if known_wrapper(self.program, id.name.as_str()));
            if !wrapper
                && call
                    .arguments
                    .iter()
                    .filter_map(|argument| argument.as_expression())
                    .any(|argument| config_receiver(argument, &self.bindings))
            {
                self.unread = true;
            }
            walk::walk_call_expression(self, call);
        }
    }
    // Runner JSON configs have no exported expression to follow.
    if serde_json::from_str::<serde_json::Value>(text).is_ok_and(|value| value.is_object()) {
        return false;
    }
    super::super::parse::parse_on_worker(
        Path::new("runner.config.ts"),
        text,
        |_, source, allocator| {
            let parsed = Parser::new(allocator, source, SourceType::ts()).parse();
            if !parsed.errors.is_empty() {
                return true;
            }
            struct Exports<'p, 'a> {
                program: &'p oxc_ast::ast::Program<'a>,
                bindings: Vec<String>,
                unread: bool,
            }
            impl<'a> Visit<'a> for Exports<'_, 'a> {
                fn visit_assignment_expression(
                    &mut self,
                    assignment: &oxc_ast::ast::AssignmentExpression<'a>,
                ) {
                    if commonjs(&assignment.left) {
                        self.unread |=
                            !plain_config(&assignment.right, self.program, &mut self.bindings);
                    }
                    walk::walk_assignment_expression(self, assignment);
                }
                fn visit_export_default_declaration(
                    &mut self,
                    export: &oxc_ast::ast::ExportDefaultDeclaration<'a>,
                ) {
                    self.unread |= export.declaration.as_expression().is_none_or(|expression| {
                        !plain_config(expression, self.program, &mut self.bindings)
                    });
                    walk::walk_export_default_declaration(self, export);
                }
                fn visit_ts_export_assignment(
                    &mut self,
                    export: &oxc_ast::ast::TSExportAssignment<'a>,
                ) {
                    self.unread |=
                        !plain_config(&export.expression, self.program, &mut self.bindings);
                    walk::walk_ts_export_assignment(self, export);
                }
                fn visit_export_named_declaration(
                    &mut self,
                    export: &oxc_ast::ast::ExportNamedDeclaration<'a>,
                ) {
                    for specifier in &export.specifiers {
                        if specifier.exported.name() == "default" {
                            self.unread |= export.source.is_some()
                                || !plain_binding(
                                    specifier.local.name().as_str(),
                                    self.program,
                                    &mut self.bindings,
                                );
                        }
                    }
                    walk::walk_export_named_declaration(self, export);
                }
            }
            let mut exports = Exports {
                program: &parsed.program,
                bindings: Vec::new(),
                unread: false,
            };
            exports.visit_program(&parsed.program);
            if exports.unread {
                return true;
            }
            let mut writes = Writes {
                program: &parsed.program,
                bindings: exports.bindings,
                unread: false,
            };
            writes.visit_program(&parsed.program);
            writes.unread
        },
    )
    .unwrap_or(true)
}
