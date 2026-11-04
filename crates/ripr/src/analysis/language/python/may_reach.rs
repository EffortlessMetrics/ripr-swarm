//! An in-package "may reach" call graph for Python, built from annotations
//! (#11425, RIPR-SPEC-0028).
//!
//! A library's own helper (`measure_table` in click) is reached by its tests
//! only through the public API (`CliRunner.invoke` -> ... -> `Context.get_help`
//! -> `self.command.get_help(self)` -> `ctx.make_formatter()` ->
//! `formatter.write_dl(...)`). Name matching cannot follow that: three hops go
//! through a parameter, an attribute or a local bound from a call. What does
//! resolve is annotation-driven: `ctx: Context` parameters, `-> HelpFormatter`
//! return annotations, `self.command: Command` attribute annotations, a local
//! bound once to an in-package constructor, and `self.method` dispatch taken
//! as a union over the class and its in-package subclasses.
//!
//! This is the graph only (PR 1 of #11425). Nothing reads it for a verdict:
//! it adds no relation, no `related_tests` row and no exposure class, and
//! whatever later consumes it can only produce weak reach, never `exposed`.
//! An edge exists only when every hop resolves unambiguously inside the
//! workspace. An edge is dropped on:
//!
//! - an untyped receiver (a method name that happens to be unique, or defined
//!   on several unrelated classes, is package-level coincidence #9226 refused;
//!   both are counted so the report can show what was refused);
//! - a type that is not an in-package class, a hop outside the workspace, an
//!   unresolved or ambiguous (multiply bound) name, or a class with an
//!   external base that could supply the method;
//! - `getattr`, `super()`, a decorator on a function or class in the chain, a
//!   class with a metaclass, `__new__` or `__getattr__`;
//! - a union wider than [`MAX_UNION`];
//! - everything `call_chain.rs` already refuses on the workspace: a file that
//!   reflects, tampers with the import system, writes a dunder attribute,
//!   spells names Python would normalise, or shares its module name with
//!   another file, and a callable some other file rebinds or patches.
//!
//! [`GuardMode::Observe`] builds the same structure without those file-level
//! guards so the report can show how many edges they cost. It exists for
//! measurement; [`GuardMode::Enforce`] is the contract.

use super::call_chain::has_dynamic_word;
use super::module_constants::ModuleBindingFacts;
use super::module_value::{
    ForeignIndex, ForeignWrites, PriorFileFacts, mentions_reflection, names_may_differ,
    names_stdlib_module,
};
use super::source_facts::parse_module_result;
use rustpython_parser::ast::{self, Expr, Mod, Ranged, Stmt};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

/// The widest override union one call site may take before the edge is
/// dropped.
const MAX_UNION: usize = 16;

/// The deepest import re-export chain followed to a definition.
const MAX_IMPORT_HOPS: usize = 4;

/// Whether the file-level guards of `call_chain.rs` apply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GuardMode {
    /// A refused file contributes no edge. The contract.
    Enforce,
    /// File-level guards are recorded but not applied. Measurement only.
    Observe,
}

/// One call site's contribution to an edge.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct EdgeInfo {
    /// First and last source line of each call expression that resolves here.
    pub(super) sites: Vec<(usize, usize)>,
    /// Whether any site reached this callee as one of several candidates (an
    /// override union) rather than as the only candidate.
    pub(super) union: bool,
}

/// What the build saw. Every call expression in an admitted callable lands in
/// exactly one of `resolved_sites` or `refusals`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct MayReachStats {
    pub(super) files: usize,
    /// File path -> the first guard that refuses it.
    pub(super) refused_files: BTreeMap<String, &'static str>,
    pub(super) callables: usize,
    pub(super) call_sites: usize,
    pub(super) resolved_sites: usize,
    pub(super) union_sites: usize,
    /// Call-level reason a site produced no edge.
    pub(super) refusals: BTreeMap<&'static str, usize>,
    /// Candidate-level reason a candidate of an otherwise resolved site was
    /// dropped.
    pub(super) candidate_refusals: BTreeMap<&'static str, usize>,
}

impl MayReachStats {
    fn refuse(&mut self, reason: &'static str) {
        *self.refusals.entry(reason).or_default() += 1;
    }

    fn refuse_candidate(&mut self, reason: &'static str) {
        *self.candidate_refusals.entry(reason).or_default() += 1;
    }
}

/// The may-reach graph: callable id (`<file>::<Qualified.name>`) to the
/// callables it may call.
#[derive(Clone, Debug, Default)]
pub(super) struct MayReachGraph {
    edges: BTreeMap<String, BTreeMap<String, EdgeInfo>>,
    pub(super) stats: MayReachStats,
}

impl MayReachGraph {
    pub(super) fn edge_count(&self) -> usize {
        self.edges.values().map(BTreeMap::len).sum()
    }

    pub(super) fn union_edge_count(&self) -> usize {
        self.edges
            .values()
            .flat_map(BTreeMap::values)
            .filter(|edge| edge.union)
            .count()
    }

    /// Every edge as `(caller, callee, info)`, in id order.
    pub(super) fn edges(&self) -> impl Iterator<Item = (&str, &str, &EdgeInfo)> {
        self.edges.iter().flat_map(|(caller, callees)| {
            callees
                .iter()
                .map(move |(callee, info)| (caller.as_str(), callee.as_str(), info))
        })
    }

    pub(super) fn has_edge(&self, caller: &str, callee: &str) -> bool {
        self.edges
            .get(caller)
            .is_some_and(|callees| callees.contains_key(callee))
    }

    /// Whether `to` may be reached from `from` through at most `max_depth`
    /// edges. A callable does not reach itself without a cycle.
    pub(super) fn reaches(&self, from: &str, to: &str, max_depth: usize) -> bool {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        let mut queue: VecDeque<(&str, usize)> = VecDeque::new();
        queue.push_back((from, 0));
        while let Some((node, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }
            let Some(callees) = self.edges.get(node) else {
                continue;
            };
            for callee in callees.keys() {
                if callee == to {
                    return true;
                }
                if seen.insert(callee.as_str()) {
                    queue.push_back((callee.as_str(), depth + 1));
                }
            }
        }
        false
    }
}

/// Builds the graph over `sources` (workspace-relative path, text). Tests and
/// production files are both indexed: the foreign-write guards need every
/// file, and a test function is a legitimate caller node.
pub(super) fn build(sources: &[(PathBuf, String)], mode: GuardMode) -> MayReachGraph {
    let parsed: Vec<Option<Vec<Stmt>>> = sources
        .iter()
        .map(|(file, source)| match parse_module_result(file, source) {
            Ok(Mod::Module(module)) => Some(module.body),
            _ => None,
        })
        .collect();
    let prior: BTreeMap<PathBuf, PriorFileFacts> = BTreeMap::new();
    let foreign = ForeignIndex::build_with_prior(
        sources
            .iter()
            .map(|(file, source)| (file.as_path(), source.as_str())),
        false,
        &prior,
    );
    let mut index = Index::new(sources, &parsed, &foreign, mode);
    index.collect();
    index.link();
    index.graph()
}

type ClassId = (usize, String);

/// The callables a call may run, with the reason each dropped candidate of an
/// otherwise resolved call was dropped; or why the call resolves to none.
type Resolved = Result<(Vec<(usize, String)>, Vec<&'static str>), &'static str>;

#[derive(Clone, Copy)]
struct FnDef<'a> {
    args: &'a ast::Arguments,
    body: &'a [Stmt],
    returns: Option<&'a Expr>,
    decorated: bool,
}

struct Class<'a> {
    bases: Vec<&'a Expr>,
    /// A decorator, a keyword (metaclass), `__new__` or `__getattr__`: what an
    /// instance is or has is not what the class body says.
    opaque: bool,
    methods: BTreeMap<String, FnDef<'a>>,
    duplicated: BTreeSet<String>,
    /// Attribute -> every annotation that types it, with the module that
    /// resolves it. `None` is an assignment of something untyped, which
    /// leaves the attribute's type unknown.
    attrs: BTreeMap<String, Vec<(usize, Option<&'a Expr>)>>,
}

enum Import {
    Symbol { module: usize, name: String },
    Module(usize),
    External,
}

struct Module<'a> {
    file: &'a Path,
    line_starts: Vec<usize>,
    functions: BTreeMap<String, FnDef<'a>>,
    classes: BTreeMap<String, Class<'a>>,
    imports: BTreeMap<String, ImportStmt>,
    /// Names bound more than once at module level (including conditional
    /// bindings): a name that may mean either of two things means neither.
    ambiguous: BTreeSet<String>,
    refusal: Option<&'static str>,
}

/// An import as written; resolved to a [`Import`] on use.
struct ImportStmt {
    level: usize,
    module: String,
    name: Option<String>,
}

enum Sym {
    Function(usize, String),
    Class(ClassId),
    Module(usize),
    External,
    Ambiguous,
}

enum Lookup {
    Found(Vec<(ClassId, String)>),
    NotFound,
    /// An external base could define the method before the in-package ones.
    Blocked,
}

struct Index<'a> {
    sources: &'a [(PathBuf, String)],
    foreign: &'a ForeignIndex<'a>,
    mode: GuardMode,
    modules: Vec<Option<Module<'a>>>,
    parsed: &'a [Option<Vec<Stmt>>],
    by_components: BTreeMap<Vec<String>, Vec<usize>>,
    subclasses: BTreeMap<ClassId, Vec<ClassId>>,
    method_owners: BTreeMap<String, Vec<ClassId>>,
    writes: BTreeMap<usize, ForeignWrites>,
    edges: BTreeMap<String, BTreeMap<String, EdgeInfo>>,
    stats: MayReachStats,
}

/// A walked statement or expression event, in source order.
enum Ev<'a> {
    Expr(&'a Expr),
    Bind(String),
    Assign { target: &'a Expr, value: &'a Expr },
    Ann { target: &'a Expr, ann: &'a Expr },
}

impl<'a> Index<'a> {
    fn new(
        sources: &'a [(PathBuf, String)],
        parsed: &'a [Option<Vec<Stmt>>],
        foreign: &'a ForeignIndex<'a>,
        mode: GuardMode,
    ) -> Self {
        Self {
            sources,
            foreign,
            mode,
            modules: Vec::new(),
            parsed,
            by_components: BTreeMap::new(),
            subclasses: BTreeMap::new(),
            method_owners: BTreeMap::new(),
            writes: BTreeMap::new(),
            edges: BTreeMap::new(),
            stats: MayReachStats::default(),
        }
    }

    fn module(&self, index: usize) -> Option<&Module<'a>> {
        self.modules.get(index).and_then(Option::as_ref)
    }

    // ----- collection -----

    fn collect(&mut self) {
        self.stats.files = self.sources.len();
        for (at, (file, source)) in self.sources.iter().enumerate() {
            let components = module_components(file);
            self.by_components.entry(components).or_default().push(at);
            let writes = self.foreign.excluding(file);
            let module = self.parsed.get(at).and_then(Option::as_ref).map(|body| {
                let mut module = collect_module(at, file, source, body);
                module.refusal = file_refusal(source, body, &writes);
                module
            });
            if let Some(module) = &module
                && let Some(reason) = module.refusal
            {
                self.stats
                    .refused_files
                    .insert(file.display().to_string(), reason);
            } else if module.is_none() {
                self.stats
                    .refused_files
                    .insert(file.display().to_string(), "parse_error");
            }
            self.writes.insert(at, writes);
            self.modules.push(module);
        }
    }

    /// Subclass and method-owner indexes, built once every module is known.
    fn link(&mut self) {
        let mut subclasses: BTreeMap<ClassId, Vec<ClassId>> = BTreeMap::new();
        let mut method_owners: BTreeMap<String, Vec<ClassId>> = BTreeMap::new();
        for (at, module) in self.modules.iter().enumerate() {
            let Some(module) = module else { continue };
            for (name, class) in &module.classes {
                let id = (at, name.clone());
                for base in &class.bases {
                    if let Some(base_id) = self.class_of_expr(at, base) {
                        subclasses.entry(base_id).or_default().push(id.clone());
                    }
                }
                for method in class.methods.keys() {
                    method_owners
                        .entry(method.clone())
                        .or_default()
                        .push(id.clone());
                }
            }
        }
        self.subclasses = subclasses;
        self.method_owners = method_owners;
    }

    // ----- modules and names -----

    fn resolve_module(&self, from: usize, import: &ImportStmt) -> Option<usize> {
        let parts: Vec<String> = import
            .module
            .split('.')
            .filter(|part| !part.is_empty())
            .map(str::to_string)
            .collect();
        if import.level == 0 {
            // An absolute import names a top-level module: the workspace root
            // or a `src` layout root. A standard-library name is never the
            // workspace's (`import json` is not `flask/json/__init__.py`);
            // suffix matching would conflate the two.
            if parts.first().is_none_or(|first| names_stdlib_module(first)) {
                return None;
            }
            let mut found: BTreeSet<usize> = BTreeSet::new();
            let rooted: Vec<String> = std::iter::once("src".to_string())
                .chain(parts.iter().cloned())
                .collect();
            for key in [&parts, &rooted] {
                found.extend(self.by_components.get(key).into_iter().flatten().copied());
            }
            return (found.len() == 1).then(|| found.iter().next().copied())?;
        }
        let file = &self.sources.get(from)?.0;
        let mut base = module_components(file);
        // A plain module sits in its package; `__init__` is the package.
        if file.file_stem().is_none_or(|stem| stem != "__init__") {
            base.pop();
        }
        for _ in 1..import.level {
            base.pop()?;
        }
        base.extend(parts);
        let found = self.by_components.get(&base)?;
        (found.len() == 1).then(|| found.first().copied())?
    }

    fn defines(&self, module: usize, name: &str) -> bool {
        self.module(module).is_some_and(|module| {
            module.functions.contains_key(name)
                || module.classes.contains_key(name)
                || module.imports.contains_key(name)
        })
    }

    fn binding(&self, module: usize, name: &str) -> Import {
        let Some(stmt) = self.module(module).and_then(|m| m.imports.get(name)) else {
            return Import::External;
        };
        let target = self.resolve_module(module, stmt);
        let Some(imported) = &stmt.name else {
            return target.map_or(Import::External, Import::Module);
        };
        if let Some(target) = target
            && self.defines(target, imported)
        {
            return Import::Symbol {
                module: target,
                name: imported.clone(),
            };
        }
        // `from pkg import submodule`.
        let sub = ImportStmt {
            level: stmt.level,
            module: if stmt.module.is_empty() {
                imported.clone()
            } else {
                format!("{}.{imported}", stmt.module)
            },
            name: None,
        };
        self.resolve_module(module, &sub)
            .map_or(Import::External, Import::Module)
    }

    /// What a bare name means at module level of `module`.
    fn symbol(&self, module: usize, name: &str, hops: usize) -> Sym {
        let Some(info) = self.module(module) else {
            return Sym::External;
        };
        if info.ambiguous.contains(name) {
            return Sym::Ambiguous;
        }
        if info.functions.contains_key(name) {
            return Sym::Function(module, name.to_string());
        }
        if info.classes.contains_key(name) {
            return Sym::Class((module, name.to_string()));
        }
        if !info.imports.contains_key(name) || hops >= MAX_IMPORT_HOPS {
            return Sym::External;
        }
        match self.binding(module, name) {
            Import::Module(target) => Sym::Module(target),
            Import::Symbol { module, name } => self.symbol(module, &name, hops + 1),
            Import::External => Sym::External,
        }
    }

    /// `name` or `module.name` as a definition, from `module`'s point of view.
    fn symbol_of_expr(&self, module: usize, expr: &Expr) -> Sym {
        match expr {
            Expr::Name(name) => self.symbol(module, name.id.as_str(), 0),
            Expr::Attribute(attribute) => match self.symbol_of_expr(module, &attribute.value) {
                Sym::Module(target) => self.symbol(target, attribute.attr.as_str(), 0),
                Sym::Ambiguous => Sym::Ambiguous,
                _ => Sym::External,
            },
            _ => Sym::External,
        }
    }

    fn class_of_expr(&self, module: usize, expr: &Expr) -> Option<ClassId> {
        match self.symbol_of_expr(module, expr) {
            Sym::Class(id) => Some(id),
            _ => None,
        }
    }

    /// The in-package class a type annotation names, through `Optional[X]`,
    /// `X | None` and a quoted forward reference. Anything wider is not a
    /// single class.
    fn class_of_annotation(&self, module: usize, annotation: &Expr) -> Option<ClassId> {
        match annotation {
            Expr::Name(_) | Expr::Attribute(_) => self.class_of_expr(module, annotation),
            Expr::Constant(constant) => {
                let ast::Constant::Str(text) = &constant.value else {
                    return None;
                };
                let mut parts = text.split('.');
                let first = parts.next()?;
                if !is_identifier(first) {
                    return None;
                }
                let mut sym = self.symbol(module, first, 0);
                for part in parts {
                    if !is_identifier(part) {
                        return None;
                    }
                    sym = match sym {
                        Sym::Module(target) => self.symbol(target, part, 0),
                        _ => return None,
                    };
                }
                match sym {
                    Sym::Class(id) => Some(id),
                    _ => None,
                }
            }
            Expr::BinOp(binop) if matches!(binop.op, ast::Operator::BitOr) => {
                match (is_none(&binop.left), is_none(&binop.right)) {
                    (false, true) => self.class_of_annotation(module, &binop.left),
                    (true, false) => self.class_of_annotation(module, &binop.right),
                    _ => None,
                }
            }
            Expr::Subscript(subscript) if is_optional(&subscript.value) => {
                self.class_of_annotation(module, &subscript.slice)
            }
            _ => None,
        }
    }

    // ----- class hierarchy -----

    fn class(&self, id: &ClassId) -> Option<&Class<'a>> {
        self.module(id.0)?.classes.get(&id.1)
    }

    fn all_subclasses(&self, id: &ClassId) -> Vec<ClassId> {
        let mut seen: BTreeSet<ClassId> = BTreeSet::new();
        let mut queue: VecDeque<ClassId> = VecDeque::new();
        queue.push_back(id.clone());
        while let Some(current) = queue.pop_front() {
            for sub in self.subclasses.get(&current).into_iter().flatten() {
                if seen.insert(sub.clone()) {
                    queue.push_back(sub.clone());
                }
            }
        }
        seen.remove(id);
        seen.into_iter().collect()
    }

    fn related(&self, a: &ClassId, b: &ClassId) -> bool {
        a == b || self.all_subclasses(a).contains(b) || self.all_subclasses(b).contains(a)
    }

    /// The nearest definitions of `method` on `id` and its in-package bases.
    fn lookup_method(&self, id: &ClassId, method: &str, depth: usize) -> Lookup {
        let Some(class) = self.class(id) else {
            return Lookup::NotFound;
        };
        if class.duplicated.contains(method) {
            return Lookup::Blocked;
        }
        if class.methods.contains_key(method) {
            return Lookup::Found(vec![(id.clone(), method.to_string())]);
        }
        if depth >= 16 {
            return Lookup::Blocked;
        }
        let mut found = Vec::new();
        for base in &class.bases {
            if self.inert_base(id.0, base) {
                continue;
            }
            let Some(base_id) = self.class_of_expr(id.0, base) else {
                return Lookup::Blocked;
            };
            match self.lookup_method(&base_id, method, depth + 1) {
                Lookup::Found(more) => found.extend(more),
                Lookup::NotFound => {}
                Lookup::Blocked => return Lookup::Blocked,
            }
        }
        if found.is_empty() {
            Lookup::NotFound
        } else {
            Lookup::Found(found)
        }
    }

    /// Bases that define no behaviour a call could land on.
    fn inert_base(&self, module: usize, base: &Expr) -> bool {
        let head = match base {
            Expr::Subscript(subscript) => &*subscript.value,
            other => other,
        };
        let name = match head {
            Expr::Name(name) => name.id.as_str(),
            Expr::Attribute(attribute) => attribute.attr.as_str(),
            _ => return false,
        };
        matches!(name, "object" | "ABC" | "Generic" | "Protocol")
            && self.class_of_expr(module, head).is_none()
    }

    /// The type of attribute `attr` on instances of `id`, when every
    /// annotation along the nearest defining class agrees.
    fn attribute_type(&self, id: &ClassId, attr: &str, depth: usize) -> Option<ClassId> {
        let class = self.class(id)?;
        if class.opaque {
            return None;
        }
        if let Some(entries) = class.attrs.get(attr) {
            let mut types = entries.iter().map(|(module, annotation)| {
                annotation.and_then(|annotation| self.class_of_annotation(*module, annotation))
            });
            let first = types.next()??;
            return types
                .all(|other| other.as_ref() == Some(&first))
                .then_some(first);
        }
        if depth >= 16 {
            return None;
        }
        let mut found: Option<ClassId> = None;
        for base in &class.bases {
            let Some(base_id) = self.class_of_expr(id.0, base) else {
                continue;
            };
            if let Some(ty) = self.attribute_type(&base_id, attr, depth + 1) {
                match &found {
                    Some(existing) if *existing != ty => return None,
                    _ => found = Some(ty),
                }
            }
        }
        found
    }

    // ----- graph -----

    fn node_id(&self, module: usize, qualified: &str) -> String {
        let file = self
            .sources
            .get(module)
            .map(|(file, _)| file.display().to_string())
            .unwrap_or_default();
        format!("{file}::{qualified}")
    }

    fn graph(&mut self) -> MayReachGraph {
        let module_count = self.modules.len();
        for at in 0..module_count {
            let Some(module) = self.module(at) else {
                continue;
            };
            if module.refusal.is_some() && self.mode == GuardMode::Enforce {
                continue;
            }
            // Collected up front: the callable bodies borrow the parsed tree,
            // not the index, so the index can be written while they are read.
            let mut callables: Vec<(String, Option<String>, FnDef<'a>)> = Vec::new();
            for (name, def) in &module.functions {
                callables.push((name.clone(), None, *def));
            }
            for (class_name, class) in &module.classes {
                if class.opaque {
                    continue;
                }
                for (method, def) in &class.methods {
                    if class.duplicated.contains(method) {
                        continue;
                    }
                    callables.push((
                        format!("{class_name}.{method}"),
                        Some(class_name.clone()),
                        *def,
                    ));
                }
            }
            for (qualified, class, def) in callables {
                if def.decorated {
                    continue;
                }
                self.stats.callables += 1;
                self.walk_callable(at, &qualified, class.as_deref(), def);
            }
        }
        MayReachGraph {
            edges: std::mem::take(&mut self.edges),
            stats: std::mem::take(&mut self.stats),
        }
    }

    fn walk_callable(&mut self, at: usize, qualified: &str, class: Option<&str>, def: FnDef<'a>) {
        let mut events: Vec<Ev<'a>> = Vec::new();
        walk_stmts(def.body, &mut events);
        let locals = self.local_types(at, class, def, &events);
        let caller = self.node_id(at, qualified);
        let scope = Scope { module: at, locals };
        for event in &events {
            let Ev::Expr(Expr::Call(call)) = event else {
                continue;
            };
            self.stats.call_sites += 1;
            let range = call.range();
            let module = self.module(at);
            let span = module.map_or((0, 0), |module| {
                (
                    line_of(&module.line_starts, usize::from(range.start())),
                    line_of(&module.line_starts, usize::from(range.end())),
                )
            });
            match self.resolve_call(&scope, &call.func) {
                Ok((candidates, dropped)) => {
                    for reason in dropped {
                        self.stats.refuse_candidate(reason);
                    }
                    let union = candidates.len() > 1;
                    self.stats.resolved_sites += 1;
                    if union {
                        self.stats.union_sites += 1;
                    }
                    for (module, qualified) in candidates {
                        let callee = self.node_id(module, &qualified);
                        let edge = self
                            .edges
                            .entry(caller.clone())
                            .or_default()
                            .entry(callee)
                            .or_default();
                        edge.sites.push(span);
                        edge.union |= union;
                    }
                }
                Err(reason) => self.stats.refuse(reason),
            }
        }
    }

    /// The type of each local that is bound exactly once.
    fn local_types(
        &self,
        at: usize,
        class: Option<&str>,
        def: FnDef<'a>,
        events: &[Ev<'a>],
    ) -> BTreeMap<String, Option<ClassId>> {
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        let mut opaque = false;
        let parameters = parameters_of(def.args);
        for (name, _) in &parameters {
            *counts.entry(name.clone()).or_default() += 1;
        }
        for event in events {
            match event {
                Ev::Bind(name) => {
                    opaque |= name.starts_with('*');
                    *counts.entry(name.clone()).or_default() += 1;
                }
                Ev::Assign { target, .. } | Ev::Ann { target, .. } => {
                    if let Expr::Name(name) = target {
                        *counts.entry(name.id.to_string()).or_default() += 1;
                    }
                }
                Ev::Expr(_) => {}
            }
        }
        let mut locals: BTreeMap<String, Option<ClassId>> = BTreeMap::new();
        for name in counts.keys() {
            locals.insert(name.clone(), None);
        }
        if opaque {
            return locals;
        }
        let single = |name: &str| counts.get(name) == Some(&1);
        for (position, (name, annotation)) in parameters.iter().enumerate() {
            if !single(name) {
                continue;
            }
            let ty = match annotation {
                Some(annotation) => self.class_of_annotation(at, annotation),
                None if position == 0 && class.is_some() => {
                    class.map(|class| (at, class.to_string()))
                }
                None => None,
            };
            locals.insert(name.clone(), ty);
        }
        for event in events {
            match event {
                Ev::Ann { target, ann } => {
                    if let Expr::Name(name) = target
                        && single(name.id.as_str())
                    {
                        locals.insert(name.id.to_string(), self.class_of_annotation(at, ann));
                    }
                }
                Ev::Assign { target, value } => {
                    if let Expr::Name(name) = target
                        && single(name.id.as_str())
                    {
                        let scope = Scope {
                            module: at,
                            locals: locals.clone(),
                        };
                        let ty = self.type_of(&scope, value);
                        locals.insert(name.id.to_string(), ty);
                    }
                }
                _ => {}
            }
        }
        locals
    }

    /// The in-package class of the instance `expr` evaluates to.
    fn type_of(&self, scope: &Scope, expr: &Expr) -> Option<ClassId> {
        match expr {
            Expr::Name(name) => scope.locals.get(name.id.as_str())?.clone(),
            Expr::Attribute(attribute) => {
                let receiver = self.type_of(scope, &attribute.value)?;
                self.attribute_type(&receiver, attribute.attr.as_str(), 0)
            }
            Expr::Call(call) => {
                // A bare name bound inside the callable shadows the module's
                // class or function; `resolve_call` refuses it. A parameter
                // used as a receiver (`ctx.make_formatter()`) is not shadowing.
                let shadowed = root_is_local(&call.func, scope);
                // `Class(...)`: an instance of the class.
                if !shadowed && let Some(class) = self.class_of_expr(scope.module, &call.func) {
                    return (!self.class(&class)?.opaque).then_some(class);
                }
                let (candidates, _) = self.resolve_call(scope, &call.func).ok()?;
                self.agreed_return(&candidates)
            }
            _ => None,
        }
    }

    /// The class every candidate's return annotation names, when they agree.
    fn agreed_return(&self, candidates: &[(usize, String)]) -> Option<ClassId> {
        let mut agreed: Option<ClassId> = None;
        for (module, qualified) in candidates {
            let def = self.callable_def(*module, qualified)?;
            let returns = def.returns?;
            let ty = self.class_of_annotation(*module, returns)?;
            match &agreed {
                Some(existing) if *existing != ty => return None,
                _ => agreed = Some(ty),
            }
        }
        agreed
    }

    fn callable_def(&self, module: usize, qualified: &str) -> Option<FnDef<'a>> {
        let module = self.module(module)?;
        match qualified.split_once('.') {
            None => module.functions.get(qualified).copied(),
            Some((class, method)) => module.classes.get(class)?.methods.get(method).copied(),
        }
    }

    /// The callables a call to `func` may run, or why it resolves to none.
    fn resolve_call(&self, scope: &Scope, func: &Expr) -> Resolved {
        match func {
            Expr::Name(name) => {
                let name = name.id.as_str();
                if matches!(name, "getattr" | "setattr" | "delattr" | "__import__") {
                    return Err("getattr");
                }
                if scope.locals.contains_key(name) {
                    return Err("local_callable");
                }
                self.call_symbol(self.symbol(scope.module, name, 0))
            }
            Expr::Attribute(attribute) => {
                let method = attribute.attr.as_str();
                if let Expr::Call(inner) = &*attribute.value
                    && matches!(&*inner.func, Expr::Name(name) if name.id.as_str() == "super")
                {
                    return Err("super_call");
                }
                if let Expr::Call(inner) = &*attribute.value
                    && matches!(&*inner.func, Expr::Name(name) if name.id.as_str() == "getattr")
                {
                    return Err("getattr");
                }
                // `module.func()` and `Class.method()`.
                if let Expr::Name(receiver) = &*attribute.value
                    && !scope.locals.contains_key(receiver.id.as_str())
                {
                    match self.symbol(scope.module, receiver.id.as_str(), 0) {
                        Sym::Module(target) => {
                            return self.call_symbol(self.symbol(target, method, 0));
                        }
                        Sym::Class(class) => {
                            return self.dispatch(&class, method, false);
                        }
                        Sym::Ambiguous => return Err("ambiguous_name"),
                        Sym::Function(..) | Sym::External => return Err("outside_workspace"),
                    }
                }
                match self.type_of(scope, &attribute.value) {
                    Some(receiver) => self.dispatch(&receiver, method, true),
                    None => Err(self.untyped_reason(method)),
                }
            }
            Expr::Call(inner) if matches!(&*inner.func, Expr::Name(name) if name.id.as_str() == "getattr") => {
                Err("getattr")
            }
            _ => Err("unresolved_callee"),
        }
    }

    fn call_symbol(&self, sym: Sym) -> Resolved {
        match sym {
            Sym::Function(module, name) => {
                let def = self
                    .callable_def(module, &name)
                    .ok_or("unresolved_callee")?;
                self.admit(module, &name, def, &name)
                    .map(|node| (vec![node], Vec::new()))
            }
            Sym::Class(class) => {
                let info = self.class(&class).ok_or("unresolved_callee")?;
                if info.opaque {
                    return Err("opaque_class");
                }
                match self.lookup_method(&class, "__init__", 0) {
                    Lookup::Found(found) => self.admit_all(found),
                    Lookup::NotFound => Err("no_constructor_body"),
                    Lookup::Blocked => Err("external_base"),
                }
            }
            Sym::Ambiguous => Err("ambiguous_name"),
            Sym::Module(_) | Sym::External => Err("outside_workspace"),
        }
    }

    /// Method `method` on an instance of `receiver`: the nearest definition
    /// and, for instance dispatch, every in-package subclass override.
    fn dispatch(&self, receiver: &ClassId, method: &str, with_overrides: bool) -> Resolved {
        if self.class(receiver).is_some_and(|class| class.opaque) {
            return Err("opaque_class");
        }
        let mut found = match self.lookup_method(receiver, method, 0) {
            Lookup::Found(found) => found,
            Lookup::NotFound => Vec::new(),
            Lookup::Blocked => return Err("external_base"),
        };
        if with_overrides {
            for sub in self.all_subclasses(receiver) {
                let Some(class) = self.class(&sub) else {
                    continue;
                };
                if class.duplicated.contains(method) {
                    return Err("duplicate_method");
                }
                if class.methods.contains_key(method) {
                    found.push((sub, method.to_string()));
                }
            }
        }
        found.sort();
        found.dedup();
        if found.is_empty() {
            return Err("unresolved_method");
        }
        if found.len() > MAX_UNION {
            return Err("union_too_wide");
        }
        self.admit_all(found)
    }

    fn admit_all(&self, found: Vec<(ClassId, String)>) -> Resolved {
        let mut out = Vec::new();
        let mut dropped = Vec::new();
        let mut last = "unresolved_method";
        for ((module, class), method) in found {
            let qualified = format!("{class}.{method}");
            let Some(def) = self.callable_def(module, &qualified) else {
                continue;
            };
            match self.admit(module, &qualified, def, &method) {
                Ok(node) => out.push(node),
                Err(reason) => {
                    last = reason;
                    dropped.push(reason);
                }
            }
        }
        if out.is_empty() {
            Err(last)
        } else {
            Ok((out, dropped))
        }
    }

    /// Whether `def` is a callable an edge may end at.
    fn admit(
        &self,
        module: usize,
        qualified: &str,
        def: FnDef<'a>,
        leaf: &str,
    ) -> Result<(usize, String), &'static str> {
        if def.decorated {
            return Err("decorator");
        }
        let info = self.module(module).ok_or("outside_workspace")?;
        if self.mode == GuardMode::Enforce {
            if info.refusal.is_some() {
                return Err("guard_refused_file");
            }
            if let Some(writes) = self.writes.get(&module) {
                let file = info.file;
                let class = qualified.split('.').next().unwrap_or(qualified);
                if writes.may_rebind(leaf, file) || writes.may_rebind(class, file) {
                    return Err("rebound_elsewhere");
                }
            }
        }
        Ok((module, qualified.to_string()))
    }

    /// Why an untyped `receiver.method(...)` produced no edge.
    fn untyped_reason(&self, method: &str) -> &'static str {
        let owners = self
            .method_owners
            .get(method)
            .map_or(&[][..], Vec::as_slice);
        match owners {
            [] => "outside_workspace",
            [_] => "untyped_unique_name",
            many => {
                let sample: Vec<&ClassId> = many.iter().take(32).collect();
                let unrelated = sample
                    .iter()
                    .any(|a| sample.iter().any(|b| !self.related(a, b)));
                if unrelated {
                    "untyped_ambiguous_name"
                } else {
                    "untyped_receiver"
                }
            }
        }
    }
}

struct Scope {
    module: usize,
    locals: BTreeMap<String, Option<ClassId>>,
}

/// Whether the leftmost name of `func` is bound inside the callable.
fn root_is_local(func: &Expr, scope: &Scope) -> bool {
    match func {
        Expr::Name(name) => scope.locals.contains_key(name.id.as_str()),
        Expr::Attribute(attribute) => root_is_local(&attribute.value, scope),
        Expr::Call(call) => root_is_local(&call.func, scope),
        _ => false,
    }
}

// ----- parsing helpers -----

/// Directory and stem components of a module path; `__init__` is its package.
fn module_components(file: &Path) -> Vec<String> {
    let mut parts: Vec<String> = file
        .parent()
        .into_iter()
        .flat_map(Path::components)
        .filter_map(|part| part.as_os_str().to_str().map(str::to_string))
        .collect();
    let stem = file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    if stem != "__init__" {
        parts.push(stem.to_string());
    }
    parts
}

fn line_of(line_starts: &[usize], offset: usize) -> usize {
    line_starts.partition_point(|start| *start <= offset)
}

fn is_identifier(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(|ch: char| ch.is_ascii_digit())
        && text
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_none(expr: &Expr) -> bool {
    matches!(expr, Expr::Constant(constant) if matches!(constant.value, ast::Constant::None))
}

fn is_optional(expr: &Expr) -> bool {
    match expr {
        Expr::Name(name) => name.id.as_str() == "Optional",
        Expr::Attribute(attribute) => attribute.attr.as_str() == "Optional",
        _ => false,
    }
}

/// Parameter names in call order, with their annotations.
fn parameters_of(args: &ast::Arguments) -> Vec<(String, Option<&Expr>)> {
    args.posonlyargs
        .iter()
        .chain(&args.args)
        .chain(&args.kwonlyargs)
        .map(|with_default| {
            (
                with_default.def.arg.to_string(),
                with_default.def.annotation.as_deref(),
            )
        })
        .chain(args.vararg.iter().map(|arg| (arg.arg.to_string(), None)))
        .chain(args.kwarg.iter().map(|arg| (arg.arg.to_string(), None)))
        .collect()
}

fn fn_def<'a>(
    args: &'a ast::Arguments,
    body: &'a [Stmt],
    returns: Option<&'a Expr>,
    decorators: &[Expr],
) -> FnDef<'a> {
    FnDef {
        args,
        body,
        returns,
        decorated: !decorators.is_empty(),
    }
}

fn collect_module<'a>(at: usize, file: &'a Path, source: &str, body: &'a [Stmt]) -> Module<'a> {
    let mut module = Module {
        file,
        line_starts: std::iter::once(0)
            .chain(source.match_indices('\n').map(|(at, _)| at + 1))
            .collect(),
        functions: BTreeMap::new(),
        classes: BTreeMap::new(),
        imports: BTreeMap::new(),
        ambiguous: BTreeSet::new(),
        refusal: None,
    };
    let mut bound: BTreeMap<String, usize> = BTreeMap::new();
    count_module_bindings(body, &mut bound, &mut module.imports);
    for (name, count) in &bound {
        if *count > 1 {
            module.ambiguous.insert(name.clone());
        }
    }
    for statement in body {
        match statement {
            Stmt::FunctionDef(function) => {
                module.functions.insert(
                    function.name.to_string(),
                    fn_def(
                        &function.args,
                        &function.body,
                        function.returns.as_deref(),
                        &function.decorator_list,
                    ),
                );
            }
            Stmt::AsyncFunctionDef(function) => {
                module.functions.insert(
                    function.name.to_string(),
                    fn_def(
                        &function.args,
                        &function.body,
                        function.returns.as_deref(),
                        &function.decorator_list,
                    ),
                );
            }
            Stmt::ClassDef(class) => {
                module
                    .classes
                    .insert(class.name.to_string(), collect_class(at, class));
            }
            _ => {}
        }
    }
    // An ambiguous name is neither of its meanings.
    for name in module.ambiguous.clone() {
        module.functions.remove(&name);
        module.classes.remove(&name);
        module.imports.remove(&name);
    }
    module
}

fn collect_class<'a>(at: usize, class: &'a ast::StmtClassDef) -> Class<'a> {
    let mut info = Class {
        bases: class.bases.iter().collect(),
        opaque: !class.decorator_list.is_empty() || !class.keywords.is_empty(),
        methods: BTreeMap::new(),
        duplicated: BTreeSet::new(),
        attrs: BTreeMap::new(),
    };
    for statement in &class.body {
        let (name, def) = match statement {
            Stmt::FunctionDef(function) => (
                function.name.to_string(),
                fn_def(
                    &function.args,
                    &function.body,
                    function.returns.as_deref(),
                    &function.decorator_list,
                ),
            ),
            Stmt::AsyncFunctionDef(function) => (
                function.name.to_string(),
                fn_def(
                    &function.args,
                    &function.body,
                    function.returns.as_deref(),
                    &function.decorator_list,
                ),
            ),
            Stmt::AnnAssign(assign) => {
                if let Expr::Name(target) = &*assign.target {
                    info.attrs
                        .entry(target.id.to_string())
                        .or_default()
                        .push((at, Some(&assign.annotation)));
                }
                continue;
            }
            _ => continue,
        };
        if matches!(
            name.as_str(),
            "__new__" | "__getattr__" | "__getattribute__"
        ) {
            info.opaque = true;
        }
        if info.methods.insert(name.clone(), def).is_some() {
            info.duplicated.insert(name);
        }
    }
    // `self.x: T = ...` and `self.x = param` where `param: T`.
    let methods: Vec<FnDef<'a>> = info.methods.values().copied().collect();
    for def in methods {
        let parameters = parameters_of(def.args);
        let Some((receiver, _)) = parameters.first() else {
            continue;
        };
        let mut events = Vec::new();
        walk_stmts(def.body, &mut events);
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for (name, _) in &parameters {
            *counts.entry(name.as_str()).or_default() += 1;
        }
        for event in &events {
            match event {
                Ev::Bind(name) => *counts.entry(name.as_str()).or_default() += 1,
                Ev::Assign { target, .. } | Ev::Ann { target, .. } => {
                    if let Expr::Name(name) = target {
                        *counts.entry(name.id.as_str()).or_default() += 1;
                    }
                }
                Ev::Expr(_) => {}
            }
        }
        for event in &events {
            match event {
                Ev::Ann { target, ann } => {
                    if let Some(attr) = receiver_attribute(target, receiver) {
                        info.attrs.entry(attr).or_default().push((at, Some(ann)));
                    }
                }
                Ev::Assign { target, value } => {
                    let Some(attr) = receiver_attribute(target, receiver) else {
                        continue;
                    };
                    if let Expr::Name(name) = value
                        && counts.get(name.id.as_str()) == Some(&1)
                        && let Some((_, Some(annotation))) = parameters
                            .iter()
                            .find(|(parameter, _)| parameter == name.id.as_str())
                    {
                        info.attrs
                            .entry(attr)
                            .or_default()
                            .push((at, Some(annotation)));
                    } else {
                        // Assigned something that is not a typed parameter:
                        // the declared type no longer bounds the attribute.
                        info.attrs.entry(attr).or_default().push((at, None));
                    }
                }
                _ => {}
            }
        }
    }
    info
}

fn receiver_attribute(target: &Expr, receiver: &str) -> Option<String> {
    let Expr::Attribute(attribute) = target else {
        return None;
    };
    matches!(&*attribute.value, Expr::Name(name) if name.id.as_str() == receiver)
        .then(|| attribute.attr.to_string())
}

fn count_module_bindings(
    body: &[Stmt],
    bound: &mut BTreeMap<String, usize>,
    imports: &mut BTreeMap<String, ImportStmt>,
) {
    for statement in body {
        match statement {
            Stmt::FunctionDef(function) => bump(bound, function.name.as_str()),
            Stmt::AsyncFunctionDef(function) => bump(bound, function.name.as_str()),
            Stmt::ClassDef(class) => bump(bound, class.name.as_str()),
            Stmt::Assign(assign) => {
                for target in &assign.targets {
                    bind_targets(target, bound);
                }
            }
            Stmt::AnnAssign(assign) => bind_targets(&assign.target, bound),
            Stmt::AugAssign(assign) => bind_targets(&assign.target, bound),
            Stmt::Import(import) => {
                for alias in &import.names {
                    let (name, module) = match &alias.asname {
                        Some(asname) => (asname.to_string(), alias.name.to_string()),
                        None => {
                            let head = alias.name.split('.').next().unwrap_or_default();
                            (head.to_string(), head.to_string())
                        }
                    };
                    bump(bound, &name);
                    imports.insert(
                        name,
                        ImportStmt {
                            level: 0,
                            module,
                            name: None,
                        },
                    );
                }
            }
            Stmt::ImportFrom(import) => {
                let level = import.level.map_or(0, |level| level.to_usize());
                let module = import
                    .module
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_default();
                for alias in &import.names {
                    if alias.name.as_str() == "*" {
                        // A star import binds names this pass cannot list.
                        bump(bound, "*");
                        continue;
                    }
                    let name = alias.asname.as_ref().unwrap_or(&alias.name).to_string();
                    bump(bound, &name);
                    imports.insert(
                        name,
                        ImportStmt {
                            level,
                            module: module.clone(),
                            name: Some(alias.name.to_string()),
                        },
                    );
                }
            }
            Stmt::If(branch) => {
                count_module_bindings(&branch.body, bound, imports);
                count_module_bindings(&branch.orelse, bound, imports);
            }
            Stmt::Try(branch) => {
                count_module_bindings(&branch.body, bound, imports);
                for handler in &branch.handlers {
                    let ast::ExceptHandler::ExceptHandler(handler) = handler;
                    count_module_bindings(&handler.body, bound, imports);
                }
                count_module_bindings(&branch.orelse, bound, imports);
                count_module_bindings(&branch.finalbody, bound, imports);
            }
            Stmt::With(with) => {
                for item in &with.items {
                    if let Some(vars) = &item.optional_vars {
                        bind_targets(vars, bound);
                    }
                }
                count_module_bindings(&with.body, bound, imports);
            }
            Stmt::For(looped) => {
                bind_targets(&looped.target, bound);
                count_module_bindings(&looped.body, bound, imports);
                count_module_bindings(&looped.orelse, bound, imports);
            }
            Stmt::While(looped) => {
                count_module_bindings(&looped.body, bound, imports);
                count_module_bindings(&looped.orelse, bound, imports);
            }
            _ => {}
        }
    }
}

fn bump(bound: &mut BTreeMap<String, usize>, name: &str) {
    *bound.entry(name.to_string()).or_default() += 1;
}

fn bind_targets(target: &Expr, bound: &mut BTreeMap<String, usize>) {
    match target {
        Expr::Name(name) => bump(bound, name.id.as_str()),
        Expr::Tuple(tuple) => tuple.elts.iter().for_each(|elt| bind_targets(elt, bound)),
        Expr::List(list) => list.elts.iter().for_each(|elt| bind_targets(elt, bound)),
        Expr::Starred(starred) => bind_targets(&starred.value, bound),
        _ => {}
    }
}

/// The first guard of `call_chain.rs` that refuses `source`, in its own text
/// or through the writes of every other workspace file.
fn file_refusal(source: &str, body: &[Stmt], writes: &ForeignWrites) -> Option<&'static str> {
    if has_dynamic_word(source) {
        return Some("dynamic_word");
    }
    if mentions_reflection(source) {
        return Some("reflection");
    }
    if names_may_differ(source) {
        return Some("names_may_differ");
    }
    if ModuleBindingFacts::of(source, body).assigns_dunder() {
        return Some("dunder_write");
    }
    if writes.may_replace_reader_code() {
        return Some("workspace_code_slot_write");
    }
    if writes.writes_dunder() {
        return Some("workspace_dunder_write");
    }
    if writes.reflection_elsewhere() {
        return Some("workspace_reflection");
    }
    if writes.shares_module_name() {
        return Some("shares_module_name");
    }
    if writes.names_may_differ_elsewhere() {
        return Some("workspace_names_may_differ");
    }
    None
}

// ----- statement and expression walking -----

/// Events of a callable body in source order. Nested definitions bind their
/// name and run their decorators and defaults, but their bodies are not this
/// callable's calls.
fn walk_stmts<'a>(body: &'a [Stmt], out: &mut Vec<Ev<'a>>) {
    for statement in body {
        walk_stmt(statement, out);
    }
}

fn walk_stmt<'a>(statement: &'a Stmt, out: &mut Vec<Ev<'a>>) {
    match statement {
        Stmt::FunctionDef(function) => {
            out.push(Ev::Bind(function.name.to_string()));
            nested_definition(&function.decorator_list, &function.args, out);
        }
        Stmt::AsyncFunctionDef(function) => {
            out.push(Ev::Bind(function.name.to_string()));
            nested_definition(&function.decorator_list, &function.args, out);
        }
        Stmt::ClassDef(class) => {
            out.push(Ev::Bind(class.name.to_string()));
            for decorator in class.decorator_list.iter().chain(&class.bases) {
                walk_expr(decorator, out);
            }
        }
        Stmt::Assign(assign) => {
            if let [target] = assign.targets.as_slice() {
                out.push(Ev::Assign {
                    target,
                    value: &assign.value,
                });
                if !matches!(target, Expr::Name(_)) {
                    bind_event_targets(target, out);
                }
            } else {
                for target in &assign.targets {
                    bind_event_targets(target, out);
                }
            }
            walk_expr(&assign.value, out);
        }
        Stmt::AnnAssign(assign) => {
            out.push(Ev::Ann {
                target: &assign.target,
                ann: &assign.annotation,
            });
            if let Some(value) = &assign.value {
                if let Expr::Name(_) = &*assign.target {
                    // The value types the name only through its annotation.
                }
                walk_expr(value, out);
            }
        }
        Stmt::AugAssign(assign) => {
            bind_event_targets(&assign.target, out);
            walk_expr(&assign.value, out);
        }
        Stmt::Return(statement) => {
            if let Some(value) = &statement.value {
                walk_expr(value, out);
            }
        }
        Stmt::Expr(statement) => walk_expr(&statement.value, out),
        Stmt::Raise(statement) => {
            for expr in statement.exc.iter().chain(&statement.cause) {
                walk_expr(expr, out);
            }
        }
        Stmt::Assert(statement) => {
            walk_expr(&statement.test, out);
            if let Some(message) = &statement.msg {
                walk_expr(message, out);
            }
        }
        Stmt::Delete(statement) => {
            for target in &statement.targets {
                bind_event_targets(target, out);
            }
        }
        Stmt::If(statement) => {
            walk_expr(&statement.test, out);
            walk_stmts(&statement.body, out);
            walk_stmts(&statement.orelse, out);
        }
        Stmt::While(statement) => {
            walk_expr(&statement.test, out);
            walk_stmts(&statement.body, out);
            walk_stmts(&statement.orelse, out);
        }
        Stmt::For(statement) => {
            bind_event_targets(&statement.target, out);
            walk_expr(&statement.iter, out);
            walk_stmts(&statement.body, out);
            walk_stmts(&statement.orelse, out);
        }
        Stmt::AsyncFor(statement) => {
            bind_event_targets(&statement.target, out);
            walk_expr(&statement.iter, out);
            walk_stmts(&statement.body, out);
            walk_stmts(&statement.orelse, out);
        }
        Stmt::With(statement) => {
            walk_with(&statement.items, out);
            walk_stmts(&statement.body, out);
        }
        Stmt::AsyncWith(statement) => {
            walk_with(&statement.items, out);
            walk_stmts(&statement.body, out);
        }
        Stmt::Try(statement) => {
            walk_stmts(&statement.body, out);
            walk_handlers(&statement.handlers, out);
            walk_stmts(&statement.orelse, out);
            walk_stmts(&statement.finalbody, out);
        }
        Stmt::TryStar(statement) => {
            walk_stmts(&statement.body, out);
            walk_handlers(&statement.handlers, out);
            walk_stmts(&statement.orelse, out);
            walk_stmts(&statement.finalbody, out);
        }
        Stmt::Match(statement) => {
            // Capture patterns bind names this pass does not list.
            out.push(Ev::Bind("*match".to_string()));
            walk_expr(&statement.subject, out);
            for case in &statement.cases {
                if let Some(guard) = &case.guard {
                    walk_expr(guard, out);
                }
                walk_stmts(&case.body, out);
            }
        }
        Stmt::Import(import) => {
            for alias in &import.names {
                let name = alias.asname.as_ref().map_or_else(
                    || alias.name.split('.').next().unwrap_or_default().to_string(),
                    ToString::to_string,
                );
                out.push(Ev::Bind(name));
            }
        }
        Stmt::ImportFrom(import) => {
            for alias in &import.names {
                if alias.name.as_str() == "*" {
                    out.push(Ev::Bind("*import".to_string()));
                } else {
                    out.push(Ev::Bind(
                        alias.asname.as_ref().unwrap_or(&alias.name).to_string(),
                    ));
                }
            }
        }
        Stmt::Global(statement) => {
            for name in &statement.names {
                out.push(Ev::Bind(name.to_string()));
                out.push(Ev::Bind(name.to_string()));
            }
        }
        Stmt::Nonlocal(statement) => {
            for name in &statement.names {
                out.push(Ev::Bind(name.to_string()));
                out.push(Ev::Bind(name.to_string()));
            }
        }
        _ => {}
    }
}

fn nested_definition<'a>(decorators: &'a [Expr], args: &'a ast::Arguments, out: &mut Vec<Ev<'a>>) {
    for expr in decorators {
        walk_expr(expr, out);
    }
    for with_default in args
        .posonlyargs
        .iter()
        .chain(&args.args)
        .chain(&args.kwonlyargs)
    {
        if let Some(default) = &with_default.default {
            walk_expr(default, out);
        }
    }
}

fn walk_with<'a>(items: &'a [ast::WithItem], out: &mut Vec<Ev<'a>>) {
    for item in items {
        walk_expr(&item.context_expr, out);
        if let Some(vars) = &item.optional_vars {
            bind_event_targets(vars, out);
        }
    }
}

fn walk_handlers<'a>(handlers: &'a [ast::ExceptHandler], out: &mut Vec<Ev<'a>>) {
    for handler in handlers {
        let ast::ExceptHandler::ExceptHandler(handler) = handler;
        if let Some(kind) = &handler.type_ {
            walk_expr(kind, out);
        }
        if let Some(name) = &handler.name {
            out.push(Ev::Bind(name.to_string()));
        }
        walk_stmts(&handler.body, out);
    }
}

fn bind_event_targets<'a>(target: &'a Expr, out: &mut Vec<Ev<'a>>) {
    match target {
        Expr::Name(name) => out.push(Ev::Bind(name.id.to_string())),
        Expr::Tuple(tuple) => tuple
            .elts
            .iter()
            .for_each(|elt| bind_event_targets(elt, out)),
        Expr::List(list) => list
            .elts
            .iter()
            .for_each(|elt| bind_event_targets(elt, out)),
        Expr::Starred(starred) => bind_event_targets(&starred.value, out),
        // `obj.attr = ...` and `obj[key] = ...` evaluate their receivers.
        Expr::Attribute(attribute) => walk_expr(&attribute.value, out),
        Expr::Subscript(subscript) => {
            walk_expr(&subscript.value, out);
            walk_expr(&subscript.slice, out);
        }
        _ => {}
    }
}

/// Every sub-expression, pre-order. A lambda is its own scope and is skipped.
fn walk_expr<'a>(expr: &'a Expr, out: &mut Vec<Ev<'a>>) {
    out.push(Ev::Expr(expr));
    match expr {
        Expr::BoolOp(e) => e.values.iter().for_each(|v| walk_expr(v, out)),
        Expr::NamedExpr(e) => {
            bind_event_targets(&e.target, out);
            walk_expr(&e.value, out);
        }
        Expr::BinOp(e) => {
            walk_expr(&e.left, out);
            walk_expr(&e.right, out);
        }
        Expr::UnaryOp(e) => walk_expr(&e.operand, out),
        Expr::IfExp(e) => {
            walk_expr(&e.test, out);
            walk_expr(&e.body, out);
            walk_expr(&e.orelse, out);
        }
        Expr::Dict(e) => {
            e.keys.iter().flatten().for_each(|v| walk_expr(v, out));
            e.values.iter().for_each(|v| walk_expr(v, out));
        }
        Expr::Set(e) => e.elts.iter().for_each(|v| walk_expr(v, out)),
        Expr::ListComp(e) => {
            walk_generators(&e.generators, out);
            walk_expr(&e.elt, out);
        }
        Expr::SetComp(e) => {
            walk_generators(&e.generators, out);
            walk_expr(&e.elt, out);
        }
        Expr::GeneratorExp(e) => {
            walk_generators(&e.generators, out);
            walk_expr(&e.elt, out);
        }
        Expr::DictComp(e) => {
            walk_generators(&e.generators, out);
            walk_expr(&e.key, out);
            walk_expr(&e.value, out);
        }
        Expr::Await(e) => walk_expr(&e.value, out),
        Expr::Yield(e) => {
            if let Some(value) = &e.value {
                walk_expr(value, out);
            }
        }
        Expr::YieldFrom(e) => walk_expr(&e.value, out),
        Expr::Compare(e) => {
            walk_expr(&e.left, out);
            e.comparators.iter().for_each(|v| walk_expr(v, out));
        }
        Expr::Call(e) => {
            walk_expr(&e.func, out);
            e.args.iter().for_each(|v| walk_expr(v, out));
            e.keywords.iter().for_each(|k| walk_expr(&k.value, out));
        }
        Expr::FormattedValue(e) => {
            walk_expr(&e.value, out);
            if let Some(spec) = &e.format_spec {
                walk_expr(spec, out);
            }
        }
        Expr::JoinedStr(e) => e.values.iter().for_each(|v| walk_expr(v, out)),
        Expr::Attribute(e) => walk_expr(&e.value, out),
        Expr::Subscript(e) => {
            walk_expr(&e.value, out);
            walk_expr(&e.slice, out);
        }
        Expr::Starred(e) => walk_expr(&e.value, out),
        Expr::List(e) => e.elts.iter().for_each(|v| walk_expr(v, out)),
        Expr::Tuple(e) => e.elts.iter().for_each(|v| walk_expr(v, out)),
        Expr::Slice(e) => {
            for part in [&e.lower, &e.upper, &e.step].into_iter().flatten() {
                walk_expr(part, out);
            }
        }
        Expr::Lambda(_) | Expr::Name(_) | Expr::Constant(_) => {}
    }
}

fn walk_generators<'a>(generators: &'a [ast::Comprehension], out: &mut Vec<Ev<'a>>) {
    for generator in generators {
        bind_event_targets(&generator.target, out);
        walk_expr(&generator.iter, out);
        generator.ifs.iter().for_each(|test| walk_expr(test, out));
    }
}
