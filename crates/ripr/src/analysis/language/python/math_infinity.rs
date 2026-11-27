//! Whether `math.inf` in one module is the standard library's infinity.
//!
//! The closed loop selector (#12954) accepts `best = -math.inf` as a seed
//! only when nothing in the module can replace `math` or its `inf`. One
//! top-level `import math` binds it; any other binding of the name, any
//! store or delete through it, any use of `math` other than reading one of
//! its public attributes, another import of the module, a `"math"` string
//! (a by-name lookup such as `sys.modules["math"]`) and any star import
//! refuse the seed.

use rustpython_parser::ast::{self, ExprContext, Fold};
use rustpython_parser::text_size::TextRange;
use std::path::Path;

/// One top-level `import math` and no other way for the module to bind,
/// replace or hand out `math`.
pub(super) fn stdlib_math_unrebound(source: &str) -> bool {
    let Ok(module) = super::source_facts::parse_module_result(Path::new("<math-infinity>"), source)
    else {
        return false;
    };
    let top_level = match &module {
        ast::Mod::Module(module) => module
            .body
            .iter()
            .filter(|statement| {
                matches!(statement, ast::Stmt::Import(import)
                    if import.names.iter().any(plain_math))
            })
            .count(),
        _ => 0,
    };
    let mut uses = MathUses { imports: 0 };
    let walked = uses.fold_mod(module).is_ok();
    walked && top_level == 1 && uses.imports == 1
}

fn plain_math(alias: &ast::Alias) -> bool {
    alias.name.as_str() == "math" && alias.asname.is_none()
}

/// Counts plain `import math` statements and fails on every other use of the
/// name that could bind it, write through it or pass the module on.
struct MathUses {
    imports: usize,
}

impl Fold<TextRange> for MathUses {
    type TargetU = TextRange;
    type Error = ();
    type UserContext = ();

    fn will_map_user(&mut self, _: &TextRange) {}

    fn map_user(&mut self, range: TextRange, _: ()) -> Result<TextRange, ()> {
        Ok(range)
    }

    fn fold_expr_attribute(&mut self, node: ast::ExprAttribute) -> Result<ast::ExprAttribute, ()> {
        if matches!(node.value.as_ref(), ast::Expr::Name(name) if name.id.as_str() == "math") {
            // `math.inf` read; `math.inf = 0`, `del math.inf` and loop or
            // tuple targets through `math` are stores.
            // A dunder such as `math.__dict__` hands out the writable
            // namespace, so only public attribute reads pass.
            return if node.ctx == ExprContext::Load && !node.attr.as_str().starts_with('_') {
                Ok(node)
            } else {
                Err(())
            };
        }
        ast::fold::fold_expr_attribute(self, node)
    }

    fn fold_expr_name(&mut self, node: ast::ExprName) -> Result<ast::ExprName, ()> {
        // Any bare `math` other than an attribute receiver: a store, a delete,
        // or a read that hands the module to `setattr` or `vars`.
        if node.id.as_str() == "math" {
            return Err(());
        }
        ast::fold::fold_expr_name(self, node)
    }

    fn fold_expr_constant(&mut self, node: ast::ExprConstant) -> Result<ast::ExprConstant, ()> {
        // `sys.modules["math"]`, `importlib.import_module("math")` and
        // `globals()["math"]` reach the module by name.
        if matches!(&node.value, ast::Constant::Str(text) if text.as_str() == "math") {
            return Err(());
        }
        ast::fold::fold_expr_constant(self, node)
    }

    fn fold_stmt_import(&mut self, node: ast::StmtImport) -> Result<ast::StmtImport, ()> {
        for alias in &node.names {
            let bound = alias.asname.as_ref().map_or_else(
                || alias.name.split('.').next().unwrap_or(""),
                |name| name.as_str(),
            );
            let names_math = alias.name.as_str() == "math" || alias.name.starts_with("math.");
            if plain_math(alias) {
                self.imports += 1;
            } else if bound == "math" || names_math {
                // `import math as m` exposes the same module under another
                // name, where a write would go unseen.
                return Err(());
            }
        }
        ast::fold::fold_stmt_import(self, node)
    }

    fn fold_stmt_import_from(
        &mut self,
        node: ast::StmtImportFrom,
    ) -> Result<ast::StmtImportFrom, ()> {
        let from_math = node.module.as_ref().is_some_and(|module| {
            module.as_str() == "math" || module.as_str().starts_with("math.")
        });
        if node.names.iter().any(|alias| {
            alias.name.as_str() == "*"
                || (from_math && alias.name.as_str().starts_with('_'))
                || alias.asname.as_ref().unwrap_or(&alias.name).as_str() == "math"
        }) {
            return Err(());
        }
        ast::fold::fold_stmt_import_from(self, node)
    }

    fn fold_stmt_function_def(
        &mut self,
        node: ast::StmtFunctionDef,
    ) -> Result<ast::StmtFunctionDef, ()> {
        if node.name.as_str() == "math" {
            return Err(());
        }
        ast::fold::fold_stmt_function_def(self, node)
    }

    fn fold_stmt_async_function_def(
        &mut self,
        node: ast::StmtAsyncFunctionDef,
    ) -> Result<ast::StmtAsyncFunctionDef, ()> {
        if node.name.as_str() == "math" {
            return Err(());
        }
        ast::fold::fold_stmt_async_function_def(self, node)
    }

    fn fold_stmt_class_def(&mut self, node: ast::StmtClassDef) -> Result<ast::StmtClassDef, ()> {
        if node.name.as_str() == "math" {
            return Err(());
        }
        ast::fold::fold_stmt_class_def(self, node)
    }

    fn fold_stmt_global(&mut self, node: ast::StmtGlobal) -> Result<ast::StmtGlobal, ()> {
        if node.names.iter().any(|name| name.as_str() == "math") {
            return Err(());
        }
        ast::fold::fold_stmt_global(self, node)
    }

    fn fold_stmt_nonlocal(&mut self, node: ast::StmtNonlocal) -> Result<ast::StmtNonlocal, ()> {
        if node.names.iter().any(|name| name.as_str() == "math") {
            return Err(());
        }
        ast::fold::fold_stmt_nonlocal(self, node)
    }

    fn fold_arg(&mut self, node: ast::Arg) -> Result<ast::Arg, ()> {
        if node.arg.as_str() == "math" {
            return Err(());
        }
        ast::fold::fold_arg(self, node)
    }

    fn fold_excepthandler_except_handler(
        &mut self,
        node: ast::ExceptHandlerExceptHandler,
    ) -> Result<ast::ExceptHandlerExceptHandler, ()> {
        if node
            .name
            .as_ref()
            .is_some_and(|name| name.as_str() == "math")
        {
            return Err(());
        }
        ast::fold::fold_excepthandler_except_handler(self, node)
    }

    fn fold_pattern_match_as(
        &mut self,
        node: ast::PatternMatchAs,
    ) -> Result<ast::PatternMatchAs, ()> {
        if node
            .name
            .as_ref()
            .is_some_and(|name| name.as_str() == "math")
        {
            return Err(());
        }
        ast::fold::fold_pattern_match_as(self, node)
    }

    fn fold_pattern_match_star(
        &mut self,
        node: ast::PatternMatchStar,
    ) -> Result<ast::PatternMatchStar, ()> {
        if node
            .name
            .as_ref()
            .is_some_and(|name| name.as_str() == "math")
        {
            return Err(());
        }
        ast::fold::fold_pattern_match_star(self, node)
    }

    fn fold_pattern_match_mapping(
        &mut self,
        node: ast::PatternMatchMapping,
    ) -> Result<ast::PatternMatchMapping, ()> {
        if node
            .rest
            .as_ref()
            .is_some_and(|name| name.as_str() == "math")
        {
            return Err(());
        }
        ast::fold::fold_pattern_match_mapping(self, node)
    }
}
