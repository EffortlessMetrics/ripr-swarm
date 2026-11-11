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
    impl Writes<'_, '_> {
        fn pattern_target_is_unread(
            &self,
            target: &oxc_ast::ast::AssignmentTargetMaybeDefault<'_>,
        ) -> bool {
            match target {
                oxc_ast::ast::AssignmentTargetMaybeDefault::AssignmentTargetWithDefault(
                    default,
                ) => self.target_is_unread(&default.binding, None),
                _ => self.target_is_unread(target.to_assignment_target(), None),
            }
        }

        fn member_target_is_unread(
            &self,
            member: &oxc_ast::ast::MemberExpression<'_>,
            value: Option<&Expression<'_>>,
        ) -> bool {
            use oxc_ast::ast::MemberExpression;
            let (object, key, harmless_key) = match member {
                MemberExpression::StaticMemberExpression(member) => {
                    (&member.object, Some(member.property.name.as_str()), false)
                }
                MemberExpression::ComputedMemberExpression(member) => (
                    &member.object,
                    member_key(&member.expression),
                    matches!(
                        member.expression.get_inner_expression(),
                        Expression::NumericLiteral(_)
                            | Expression::BooleanLiteral(_)
                            | Expression::NullLiteral(_)
                    ),
                ),
                MemberExpression::PrivateFieldExpression(_) => return false,
            };
            if value.is_none() && key.is_some_and(|key| export_member(object, key)) {
                return true;
            }
            config_receiver(object, &self.bindings)
                && key.map_or(!harmless_key, |key| {
                    override_key(key)
                        || (key == "test"
                            && !value.is_some_and(|value| {
                                plain_write(value, self.program, &self.bindings)
                            }))
                })
        }

        fn target_is_unread(
            &self,
            target: &oxc_ast::ast::AssignmentTarget<'_>,
            value: Option<&Expression<'_>>,
        ) -> bool {
            use oxc_ast::ast::{AssignmentTarget, AssignmentTargetProperty};
            match target {
                AssignmentTarget::AssignmentTargetIdentifier(id) => {
                    self.bindings.iter().any(|name| name == id.name.as_str())
                }
                AssignmentTarget::ComputedMemberExpression(_)
                | AssignmentTarget::StaticMemberExpression(_)
                | AssignmentTarget::PrivateFieldExpression(_) => {
                    self.member_target_is_unread(target.to_member_expression(), value)
                }
                // Pattern values cannot be inferred from the whole RHS or a
                // default: extraction may select another value. Inspect every
                // nested destination, including shorthand, defaults and rest.
                AssignmentTarget::ArrayAssignmentTarget(array) => {
                    array
                        .elements
                        .iter()
                        .flatten()
                        .any(|target| self.pattern_target_is_unread(target))
                        || array
                            .rest
                            .as_ref()
                            .is_some_and(|rest| self.target_is_unread(&rest.target, None))
                }
                AssignmentTarget::ObjectAssignmentTarget(object) => {
                    object.properties.iter().any(|property| match property {
                        AssignmentTargetProperty::AssignmentTargetPropertyIdentifier(property) => {
                            self.bindings
                                .iter()
                                .any(|name| name == property.binding.name.as_str())
                        }
                        AssignmentTargetProperty::AssignmentTargetPropertyProperty(property) => {
                            self.pattern_target_is_unread(&property.binding)
                        }
                    }) || object
                        .rest
                        .as_ref()
                        .is_some_and(|rest| self.target_is_unread(&rest.target, None))
                }
                _ => target.get_expression().is_some_and(|expression| {
                    // TypeScript assertions/non-null wrappers preserve the
                    // destination; reads within it remain ordinary expressions.
                    let expression = expression.get_inner_expression();
                    match expression {
                        Expression::Identifier(id) => {
                            self.bindings.iter().any(|name| name == id.name.as_str())
                        }
                        _ => expression
                            .as_member_expression()
                            .is_some_and(|member| self.member_target_is_unread(member, value)),
                    }
                }),
            }
        }
    }
    impl<'a> Visit<'a> for Writes<'_, 'a> {
        fn visit_assignment_expression(
            &mut self,
            assignment: &oxc_ast::ast::AssignmentExpression<'a>,
        ) {
            self.unread |= self.target_is_unread(&assignment.left, Some(&assignment.right));
            walk::walk_assignment_expression(self, assignment);
        }
        fn visit_for_statement_left(&mut self, left: &oxc_ast::ast::ForStatementLeft<'a>) {
            if let Some(target) = left.as_assignment_target() {
                self.unread |= self.target_is_unread(target, None);
            }
            walk::walk_for_statement_left(self, left);
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
