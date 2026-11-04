//! The annotation-driven may-reach graph (#11425): a resolvable chain, and a
//! negative fixture for each refusal. The negatives are written first: each
//! pins an edge a "package-level coincidence" resolver would draw and this one
//! must not.

use super::may_reach::{GuardMode, MayReachGraph, build};
use std::path::PathBuf;

type Check = Result<(), String>;

fn graph_with(files: &[(&str, &str)], mode: GuardMode) -> MayReachGraph {
    let sources: Vec<(PathBuf, String)> = files
        .iter()
        .map(|(path, text)| (PathBuf::from(path), (*text).to_string()))
        .collect();
    build(&sources, mode)
}

fn graph(files: &[(&str, &str)]) -> MayReachGraph {
    graph_with(files, GuardMode::Enforce)
}

fn ensure(condition: bool, message: impl FnOnce() -> String) -> Check {
    if condition { Ok(()) } else { Err(message()) }
}

fn dump(graph: &MayReachGraph) -> String {
    let edges: Vec<String> = graph
        .edges()
        .map(|(caller, callee, _)| format!("{caller} -> {callee}"))
        .collect();
    format!(
        "edges: {edges:?}; refusals: {:?}; candidate refusals: {:?}; refused files: {:?}",
        graph.stats.refusals, graph.stats.candidate_refusals, graph.stats.refused_files
    )
}

fn has(graph: &MayReachGraph, caller: &str, callee: &str) -> Check {
    ensure(graph.has_edge(caller, callee), || {
        format!("missing {caller} -> {callee}: {}", dump(graph))
    })
}

fn lacks(graph: &MayReachGraph, caller: &str, callee: &str) -> Check {
    ensure(!graph.has_edge(caller, callee), || {
        format!("unexpected {caller} -> {callee}: {}", dump(graph))
    })
}

fn refused(graph: &MayReachGraph, reason: &str) -> Check {
    ensure(
        graph.stats.refusals.get(reason).copied().unwrap_or(0) > 0
            || graph
                .stats
                .candidate_refusals
                .get(reason)
                .copied()
                .unwrap_or(0)
                > 0,
        || format!("no `{reason}` refusal: {}", dump(graph)),
    )
}

const FMT: &str = "\
def measure_table(rows):
    return tuple(len(row) for row in rows)


class HelpFormatter:
    def write_dl(self, rows):
        return measure_table(rows)
";

const CORE: &str = "\
from .fmt import HelpFormatter


class Context:
    def __init__(self, command: \"Command\") -> None:
        self.command = command

    def make_formatter(self) -> HelpFormatter:
        return HelpFormatter()

    def get_help(self) -> str:
        return self.command.get_help(self)


class Command:
    def get_help(self, ctx: Context) -> str:
        formatter = ctx.make_formatter()
        return self.format_help(ctx, formatter)

    def format_help(self, ctx: Context, formatter: HelpFormatter) -> str:
        formatter.write_dl([])
        return \"\"


class Group(Command):
    def format_help(self, ctx: Context, formatter: HelpFormatter) -> str:
        formatter.write_dl([[1]])
        return \"\"
";

const CHAIN: [(&str, &str); 2] = [("src/pkg/fmt.py", FMT), ("src/pkg/core.py", CORE)];

#[test]
fn a_resolvable_annotation_chain_reaches_the_helper() -> Check {
    let graph = graph(&CHAIN);
    has(
        &graph,
        "src/pkg/core.py::Context.get_help",
        "src/pkg/core.py::Command.get_help",
    )?;
    has(
        &graph,
        "src/pkg/core.py::Command.get_help",
        "src/pkg/core.py::Context.make_formatter",
    )?;
    has(
        &graph,
        "src/pkg/core.py::Command.format_help",
        "src/pkg/fmt.py::HelpFormatter.write_dl",
    )?;
    has(
        &graph,
        "src/pkg/fmt.py::HelpFormatter.write_dl",
        "src/pkg/fmt.py::measure_table",
    )?;
    ensure(
        graph.reaches(
            "src/pkg/core.py::Context.get_help",
            "src/pkg/fmt.py::measure_table",
            12,
        ),
        || {
            format!(
                "the --help chain does not reach measure_table: {}",
                dump(&graph)
            )
        },
    )?;
    ensure(
        !graph.reaches(
            "src/pkg/fmt.py::measure_table",
            "src/pkg/core.py::Context.get_help",
            12,
        ),
        || "reach must follow call direction".to_string(),
    )
}

#[test]
fn self_dispatch_is_a_union_over_in_package_subclasses() -> Check {
    let graph = graph(&CHAIN);
    // `self.format_help` inside `Command.get_help` may land on the override.
    has(
        &graph,
        "src/pkg/core.py::Command.get_help",
        "src/pkg/core.py::Command.format_help",
    )?;
    has(
        &graph,
        "src/pkg/core.py::Command.get_help",
        "src/pkg/core.py::Group.format_help",
    )?;
    ensure(graph.union_edge_count() >= 2, || dump(&graph))
}

#[test]
fn a_same_named_method_on_an_unrelated_class_is_not_an_edge() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "class Printer:\n    def render(self):\n        return 1\n\n\nclass Page:\n    def render(self):\n        return 2\n\n\ndef draw(item):\n    return item.render()\n",
    )]);
    lacks(&graph, "src/pkg/a.py::draw", "src/pkg/a.py::Printer.render")?;
    lacks(&graph, "src/pkg/a.py::draw", "src/pkg/a.py::Page.render")?;
    refused(&graph, "untyped_ambiguous_name")
}

#[test]
fn a_typed_receiver_does_not_borrow_an_unrelated_class_with_the_same_method() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "class Printer:\n    def render(self):\n        return 1\n\n\nclass Page:\n    def render(self):\n        return 2\n\n\ndef draw(item: Page):\n    return item.render()\n",
    )]);
    has(&graph, "src/pkg/a.py::draw", "src/pkg/a.py::Page.render")?;
    lacks(&graph, "src/pkg/a.py::draw", "src/pkg/a.py::Printer.render")
}

#[test]
fn a_unique_method_name_on_an_untyped_receiver_is_still_refused() -> Check {
    // The coincidence #9226 declined: only one class defines `render`, but
    // nothing says the receiver is that class.
    let graph = graph(&[(
        "src/pkg/a.py",
        "class Page:\n    def render(self):\n        return 2\n\n\ndef draw(item):\n    return item.render()\n",
    )]);
    lacks(&graph, "src/pkg/a.py::draw", "src/pkg/a.py::Page.render")?;
    refused(&graph, "untyped_unique_name")
}

#[test]
fn a_subclass_override_does_not_leak_to_a_sibling() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "class Base:\n    def run(self):\n        return 0\n\n\nclass Left(Base):\n    def run(self):\n        return 1\n\n\nclass Right(Base):\n    def run(self):\n        return 2\n\n\ndef go(item: Left):\n    return item.run()\n",
    )]);
    has(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Left.run")?;
    // `Left.run` overrides `Base.run`, so the nearest definition is `Left`'s.
    lacks(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Right.run")?;
    lacks(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Base.run")
}

#[test]
fn a_base_typed_receiver_unions_every_override() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "class Base:\n    def run(self):\n        return 0\n\n\nclass Left(Base):\n    def run(self):\n        return 1\n\n\nclass Right(Base):\n    def run(self):\n        return 2\n\n\nclass Other:\n    def run(self):\n        return 3\n\n\ndef go(item: Base):\n    return item.run()\n",
    )]);
    for target in ["Base.run", "Left.run", "Right.run"] {
        has(
            &graph,
            "src/pkg/a.py::go",
            &format!("src/pkg/a.py::{target}"),
        )?;
    }
    lacks(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Other.run")
}

#[test]
fn an_untyped_receiver_hop_breaks_the_chain() -> Check {
    let graph = graph(&[
        ("src/pkg/fmt.py", FMT),
        (
            "src/pkg/core.py",
            "from .fmt import HelpFormatter\n\n\nclass Command:\n    def get_help(self, ctx):\n        formatter = ctx.make_formatter()\n        formatter.write_dl([])\n\n\nclass Context:\n    def make_formatter(self):\n        return HelpFormatter()\n",
        ),
    ]);
    lacks(
        &graph,
        "src/pkg/core.py::Command.get_help",
        "src/pkg/core.py::Context.make_formatter",
    )?;
    lacks(
        &graph,
        "src/pkg/core.py::Command.get_help",
        "src/pkg/fmt.py::HelpFormatter.write_dl",
    )?;
    ensure(
        !graph.reaches(
            "src/pkg/core.py::Command.get_help",
            "src/pkg/fmt.py::measure_table",
            12,
        ),
        || dump(&graph),
    )
}

#[test]
fn an_unannotated_return_does_not_type_the_local() -> Check {
    let graph = graph(&[
        ("src/pkg/fmt.py", FMT),
        (
            "src/pkg/core.py",
            "from .fmt import HelpFormatter\n\n\nclass Context:\n    def make_formatter(self):\n        return HelpFormatter()\n\n\ndef help_of(ctx: Context):\n    formatter = ctx.make_formatter()\n    formatter.write_dl([])\n",
        ),
    ]);
    has(
        &graph,
        "src/pkg/core.py::help_of",
        "src/pkg/core.py::Context.make_formatter",
    )?;
    lacks(
        &graph,
        "src/pkg/core.py::help_of",
        "src/pkg/fmt.py::HelpFormatter.write_dl",
    )
}

#[test]
fn an_annotated_return_types_a_local_bound_from_a_parameter_receiver_call() -> Check {
    let graph = graph(&[
        ("src/pkg/fmt.py", FMT),
        (
            "src/pkg/core.py",
            "from .fmt import HelpFormatter\n\n\nclass Context:\n    def make_formatter(self) -> HelpFormatter:\n        return HelpFormatter()\n\n    def formatter(self) -> HelpFormatter:\n        return self.make_formatter()\n\n    def write(self):\n        f = self.make_formatter()\n        return f.write_dl([])\n\n\ndef help_of(ctx: Context):\n    formatter = ctx.make_formatter()\n    formatter.write_dl([])\n",
        ),
    ]);
    // `ctx` is a parameter, not a shadowed class: its annotated return types
    // the local.
    has(
        &graph,
        "src/pkg/core.py::help_of",
        "src/pkg/fmt.py::HelpFormatter.write_dl",
    )?;
    // The same through `self.method()`.
    has(
        &graph,
        "src/pkg/core.py::Context.write",
        "src/pkg/fmt.py::HelpFormatter.write_dl",
    )
}

#[test]
fn a_shadowed_bare_name_constructor_does_not_type_the_local() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "class Engine:\n    def start(self):\n        return 1\n\n\ndef run(Engine):\n    engine = Engine()\n    return engine.start()\n",
    )]);
    lacks(&graph, "src/pkg/a.py::run", "src/pkg/a.py::Engine.start")
}

#[test]
fn a_local_bound_twice_is_untyped() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "class Alpha:\n    def go(self):\n        return 1\n\n\nclass Beta:\n    def go(self):\n        return 2\n\n\ndef run(flag):\n    item = Alpha()\n    if flag:\n        item = Beta()\n    return item.go()\n",
    )]);
    lacks(&graph, "src/pkg/a.py::run", "src/pkg/a.py::Alpha.go")?;
    lacks(&graph, "src/pkg/a.py::run", "src/pkg/a.py::Beta.go")?;
    // The constructors themselves still resolve: no `__init__` body to enter.
    refused(&graph, "untyped_ambiguous_name")
}

#[test]
fn a_local_bound_once_to_a_constructor_is_typed() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "class Alpha:\n    def go(self):\n        return 1\n\n\nclass Beta:\n    def go(self):\n        return 2\n\n\ndef run():\n    item = Alpha()\n    return item.go()\n",
    )]);
    has(&graph, "src/pkg/a.py::run", "src/pkg/a.py::Alpha.go")?;
    lacks(&graph, "src/pkg/a.py::run", "src/pkg/a.py::Beta.go")
}

#[test]
fn a_decorator_in_the_chain_drops_the_edge() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "import functools\n\n\ndef keep(fn):\n    @functools.wraps(fn)\n    def inner(*args):\n        return fn(*args)\n    return inner\n\n\nclass Page:\n    @keep\n    def render(self):\n        return 1\n\n    def show(self):\n        return self.render()\n\n\n@keep\ndef helper():\n    return 2\n\n\ndef top(page: Page):\n    helper()\n    return page.show()\n",
    )]);
    lacks(
        &graph,
        "src/pkg/a.py::Page.show",
        "src/pkg/a.py::Page.render",
    )?;
    lacks(&graph, "src/pkg/a.py::top", "src/pkg/a.py::helper")?;
    has(&graph, "src/pkg/a.py::top", "src/pkg/a.py::Page.show")?;
    ensure(
        !graph.reaches("src/pkg/a.py::top", "src/pkg/a.py::Page.render", 8),
        || dump(&graph),
    )?;
    refused(&graph, "decorator")
}

#[test]
fn a_decorated_caller_contributes_no_edges() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "def keep(fn):\n    return fn\n\n\ndef leaf():\n    return 1\n\n\n@keep\ndef decorated():\n    return leaf()\n",
    )]);
    lacks(&graph, "src/pkg/a.py::decorated", "src/pkg/a.py::leaf")
}

#[test]
fn getattr_dispatch_is_not_an_edge() -> Check {
    // Observe mode: the file-level reflection guard is off, so the refusal
    // shown is the call-level one.
    let files = [(
        "src/pkg/a.py",
        "class Page:\n    def render(self):\n        return 1\n\n    def show(self, name):\n        return getattr(self, name)()\n\n    def direct(self):\n        return self.render()\n",
    )];
    let observed = graph_with(&files, GuardMode::Observe);
    lacks(
        &observed,
        "src/pkg/a.py::Page.show",
        "src/pkg/a.py::Page.render",
    )?;
    has(
        &observed,
        "src/pkg/a.py::Page.direct",
        "src/pkg/a.py::Page.render",
    )?;
    refused(&observed, "getattr")?;
    // Enforced: the file reflects, so even the direct call is refused.
    let enforced = graph(&files);
    lacks(
        &enforced,
        "src/pkg/a.py::Page.direct",
        "src/pkg/a.py::Page.render",
    )?;
    ensure(
        enforced.stats.refused_files.get("src/pkg/a.py") == Some(&"reflection"),
        || dump(&enforced),
    )
}

#[test]
fn reflection_in_another_workspace_file_refuses_every_file() -> Check {
    let files = [
        (
            "src/pkg/a.py",
            "def leaf():\n    return 1\n\n\ndef top():\n    return leaf()\n",
        ),
        (
            "src/pkg/b.py",
            "def peek(obj, name):\n    return getattr(obj, name)\n",
        ),
    ];
    let enforced = graph(&files);
    lacks(&enforced, "src/pkg/a.py::top", "src/pkg/a.py::leaf")?;
    ensure(
        enforced.stats.refused_files.get("src/pkg/a.py") == Some(&"workspace_reflection"),
        || dump(&enforced),
    )?;
    let observed = graph_with(&files, GuardMode::Observe);
    has(&observed, "src/pkg/a.py::top", "src/pkg/a.py::leaf")
}

#[test]
fn a_dunder_write_anywhere_refuses_the_workspace() -> Check {
    let files = [
        (
            "src/pkg/a.py",
            "def leaf():\n    return 1\n\n\ndef top():\n    return leaf()\n",
        ),
        (
            "src/pkg/b.py",
            "def tamper(cls):\n    cls.__class__ = dict\n",
        ),
    ];
    let enforced = graph(&files);
    lacks(&enforced, "src/pkg/a.py::top", "src/pkg/a.py::leaf")?;
    ensure(
        enforced.stats.refused_files.contains_key("src/pkg/a.py"),
        || dump(&enforced),
    )
}

#[test]
fn a_shadowed_module_name_refuses_both_files() -> Check {
    let files = [
        (
            "src/pkg/util.py",
            "def leaf():\n    return 1\n\n\ndef top():\n    return leaf()\n",
        ),
        ("vendor/util.py", "def other():\n    return 2\n"),
    ];
    let enforced = graph(&files);
    lacks(&enforced, "src/pkg/util.py::top", "src/pkg/util.py::leaf")?;
    ensure(
        enforced.stats.refused_files.get("src/pkg/util.py") == Some(&"shares_module_name"),
        || dump(&enforced),
    )
}

#[test]
fn a_helper_a_test_monkeypatches_is_not_an_edge_target() -> Check {
    let files = [
        (
            "src/pkg/fmt.py",
            "def measure_table(rows):\n    return len(rows)\n\n\ndef write(rows):\n    return measure_table(rows)\n",
        ),
        (
            "tests/test_fmt.py",
            "from pkg import fmt\n\n\ndef test_patched(monkeypatch):\n    monkeypatch.setattr(fmt, \"measure_table\", lambda rows: 0)\n    assert fmt.write([]) == 0\n",
        ),
    ];
    let enforced = graph(&files);
    lacks(
        &enforced,
        "src/pkg/fmt.py::write",
        "src/pkg/fmt.py::measure_table",
    )?;
    let observed = graph_with(&files, GuardMode::Observe);
    has(
        &observed,
        "src/pkg/fmt.py::write",
        "src/pkg/fmt.py::measure_table",
    )
}

#[test]
fn a_hop_outside_the_workspace_is_not_an_edge() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "import os\nfrom somewhere import Session\n\n\nclass Page:\n    def go(self, session: Session):\n        os.path.join('a', 'b')\n        session.get('x')\n        return len('x')\n",
    )]);
    ensure(graph.edge_count() == 0, || dump(&graph))?;
    refused(&graph, "outside_workspace")
}

#[test]
fn a_class_with_an_external_base_blocks_inherited_dispatch() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "from elsewhere import Mixin\n\n\nclass Page(Mixin):\n    def own(self):\n        return 1\n\n\ndef go(page: Page):\n    page.own()\n    return page.inherited()\n",
    )]);
    has(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Page.own")?;
    ensure(
        graph
            .edges()
            .all(|(_, callee, _)| !callee.ends_with("inherited")),
        || dump(&graph),
    )?;
    refused(&graph, "external_base")
}

#[test]
fn super_calls_are_not_edges() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "class Base:\n    def run(self):\n        return 0\n\n\nclass Child(Base):\n    def run(self):\n        return super().run()\n",
    )]);
    lacks(&graph, "src/pkg/a.py::Child.run", "src/pkg/a.py::Base.run")?;
    refused(&graph, "super_call")
}

#[test]
fn a_metaclass_or_new_makes_the_class_opaque() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "class Meta(type):\n    pass\n\n\nclass Page(metaclass=Meta):\n    def run(self):\n        return 0\n\n\nclass Odd:\n    def __new__(cls):\n        return object.__new__(cls)\n\n    def run(self):\n        return 1\n\n\ndef go(a: Page, b: Odd):\n    a.run()\n    return b.run()\n",
    )]);
    lacks(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Page.run")?;
    lacks(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Odd.run")?;
    refused(&graph, "opaque_class")
}

#[test]
fn a_getattr_or_getattribute_makes_the_class_opaque() -> Check {
    // Observe mode: the file-level reflection guard (which also fires on a
    // `__getattr__` definition) is off, so only the class-level guard can
    // refuse these receivers.
    let graph = graph_with(
        &[(
            "src/pkg/a.py",
            "class Lazy:\n    def __getattr__(self, name):\n        return name\n\n    def run(self):\n        return 0\n\n\nclass Hooked:\n    def __getattribute__(self, name):\n        return name\n\n    def run(self):\n        return 1\n\n\ndef go(a: Lazy, b: Hooked):\n    a.run()\n    return b.run()\n",
        )],
        GuardMode::Observe,
    );
    lacks(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Lazy.run")?;
    lacks(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Hooked.run")?;
    refused(&graph, "opaque_class")
}

fn fan_out_source(subclasses: usize) -> String {
    let mut source = String::from("class Base:\n    def run(self):\n        return 0\n");
    for index in 0..subclasses {
        source.push_str(&format!(
            "\n\nclass Sub{index}(Base):\n    def run(self):\n        return {index}\n"
        ));
    }
    source.push_str("\n\ndef go(item: Base):\n    return item.run()\n");
    source
}

#[test]
fn a_union_within_the_cap_resolves_every_override() -> Check {
    // Positive control for the cap test below: the same shape under MAX_UNION.
    let graph = graph(&[("src/pkg/a.py", &fan_out_source(3))]);
    has(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Base.run")?;
    has(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Sub2.run")
}

#[test]
fn a_union_wider_than_the_cap_is_dropped_whole() -> Check {
    // Base plus 17 overrides is 18 candidates, above MAX_UNION (16).
    let graph = graph(&[("src/pkg/a.py", &fan_out_source(17))]);
    lacks(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Base.run")?;
    lacks(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Sub0.run")?;
    refused(&graph, "union_too_wide")
}

#[test]
fn a_same_file_global_rebind_of_a_callee_is_not_an_edge() -> Check {
    let positive = graph(&[(
        "src/pkg/a.py",
        "def helper():\n    return 1\n\n\ndef go():\n    return helper()\n",
    )]);
    has(&positive, "src/pkg/a.py::go", "src/pkg/a.py::helper")?;
    let graph = graph(&[(
        "src/pkg/a.py",
        "def helper():\n    return 1\n\n\ndef patch(replacement):\n    global helper\n    helper = replacement\n\n\ndef go():\n    return helper()\n",
    )]);
    lacks(&graph, "src/pkg/a.py::go", "src/pkg/a.py::helper")?;
    // The file-level dunder/namespace-write guard already refuses a `global`
    // write; the in-file admit check is the second line behind it.
    ensure(graph.edge_count() == 0, || dump(&graph))
}

#[test]
fn a_same_file_method_replacement_is_not_an_edge() -> Check {
    let positive = graph(&[(
        "src/pkg/a.py",
        "class Page:\n    def render(self):\n        return 1\n\n\ndef go(page: Page):\n    return page.render()\n",
    )]);
    has(&positive, "src/pkg/a.py::go", "src/pkg/a.py::Page.render")?;
    let graph = graph(&[(
        "src/pkg/a.py",
        "class Page:\n    def render(self):\n        return 1\n\n\ndef other(self):\n    return 2\n\n\nPage.render = other\n\n\ndef go(page: Page):\n    return page.render()\n",
    )]);
    lacks(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Page.render")?;
    refused(&graph, "rebound_in_file")
}

#[test]
fn a_receiver_attribute_written_by_any_target_form_stops_typing_it() -> Check {
    let source = |write: &str| {
        format!(
            "class Engine:\n    def start(self):\n        return 1\n\n\nclass Car:\n    engine: Engine\n\n    def swap(self, items, source):\n        {write}\n\n    def drive(self):\n        return self.engine.start()\n"
        )
    };
    let untouched = graph(&[("src/pkg/a.py", &source("pass"))]);
    has(
        &untouched,
        "src/pkg/a.py::Car.drive",
        "src/pkg/a.py::Engine.start",
    )?;
    for write in [
        "self.engine, other = source",
        "[self.engine, other] = source",
        "*self.engine, other = source",
        "other = self.engine = source",
        "for self.engine in items:\\n            pass",
        "with source as self.engine:\\n            pass",
        "self.engine += source",
    ] {
        let write = write.replace("\\n", "\n");
        let graph = graph(&[("src/pkg/a.py", &source(&write))]);
        lacks(
            &graph,
            "src/pkg/a.py::Car.drive",
            "src/pkg/a.py::Engine.start",
        )
        .map_err(|why| format!("`{write}`: {why}"))?;
    }
    Ok(())
}

#[test]
fn a_duplicated_method_blocks_the_lookup_with_its_own_reason() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "class Page:\n    def run(self):\n        return 1\n\n    def run(self):\n        return 2\n\n\ndef go(page: Page):\n    return page.run()\n",
    )]);
    lacks(&graph, "src/pkg/a.py::go", "src/pkg/a.py::Page.run")?;
    refused(&graph, "duplicate_method")
}

#[test]
fn a_hierarchy_deeper_than_the_walk_blocks_the_lookup_with_its_own_reason() -> Check {
    let mut source = String::from("class C0:\n    def run(self):\n        return 0\n");
    for index in 1..=18 {
        source.push_str(&format!("\n\nclass C{index}(C{}):\n    pass\n", index - 1));
    }
    source.push_str("\n\ndef go(item: C18):\n    return item.run()\n");
    let graph = graph(&[("src/pkg/a.py", &source)]);
    lacks(&graph, "src/pkg/a.py::go", "src/pkg/a.py::C0.run")?;
    refused(&graph, "hierarchy_too_deep")
}

#[test]
fn a_name_bound_twice_in_a_module_is_ambiguous() -> Check {
    let graph = graph(&[(
        "src/pkg/a.py",
        "try:\n    from fast import helper\nexcept ImportError:\n    def helper():\n        return 1\n\n\ndef top():\n    return helper()\n",
    )]);
    ensure(graph.edge_count() == 0, || dump(&graph))?;
    refused(&graph, "ambiguous_name")
}

#[test]
fn a_cross_module_constructor_and_attribute_annotation_resolve() -> Check {
    let graph = graph(&[
        ("src/pkg/__init__.py", "from .engine import Engine\n"),
        (
            "src/pkg/engine.py",
            "class Engine:\n    def __init__(self):\n        self.ready = True\n\n    def start(self):\n        return self.ready\n",
        ),
        (
            "src/pkg/car.py",
            "from . import engine\nfrom pkg import Engine\n\n\nclass Car:\n    engine: Engine\n\n    def drive(self):\n        return self.engine.start()\n\n    def build(self):\n        return engine.Engine()\n",
        ),
    ]);
    has(
        &graph,
        "src/pkg/car.py::Car.drive",
        "src/pkg/engine.py::Engine.start",
    )?;
    has(
        &graph,
        "src/pkg/car.py::Car.build",
        "src/pkg/engine.py::Engine.__init__",
    )
}

#[test]
fn a_standard_library_import_is_not_the_workspace_module_of_the_same_name() -> Check {
    // flask: `import json as _json` inside the package must not resolve to
    // `src/flask/json/__init__.py`, which the corpus report caught as a false
    // self-edge.
    let graph = graph(&[(
        "src/pkg/json/__init__.py",
        "import json as _json\n\n\ndef dumps(obj):\n    return _json.dumps(obj)\n",
    )]);
    lacks(
        &graph,
        "src/pkg/json/__init__.py::dumps",
        "src/pkg/json/__init__.py::dumps",
    )?;
    ensure(graph.edge_count() == 0, || dump(&graph))
}

#[test]
fn an_absolute_import_names_a_root_module_not_a_path_suffix() -> Check {
    let graph = graph(&[
        ("src/pkg/util.py", "def helper():\n    return 1\n"),
        (
            "other/pkg/user.py",
            "from util import helper\n\n\ndef go():\n    return helper()\n",
        ),
    ]);
    // `util` is not a top-level module here (`src/pkg/util.py` is `pkg.util`).
    lacks(&graph, "other/pkg/user.py::go", "src/pkg/util.py::helper")
}

#[test]
fn graph_stats_account_for_every_call_site() -> Check {
    let graph = graph(&CHAIN);
    let refused: usize = graph.stats.refusals.values().sum();
    ensure(
        graph.stats.call_sites == graph.stats.resolved_sites + refused,
        || format!("{:?}", graph.stats),
    )
}

// ----- measured false-edge report -----

fn summary(label: &str, graph: &MayReachGraph) -> String {
    let stats = &graph.stats;
    let mut out = format!(
        "[{label}] files={} refused_files={} callables={} call_sites={} resolved_sites={} \
         union_sites={} edges={} union_edges={}\n",
        stats.files,
        stats.refused_files.len(),
        stats.callables,
        stats.call_sites,
        stats.resolved_sites,
        stats.union_sites,
        graph.edge_count(),
        graph.union_edge_count(),
    );
    for (reason, count) in &stats.refusals {
        out.push_str(&format!("[{label}] site_refusal {reason} {count}\n"));
    }
    for (reason, count) in &stats.candidate_refusals {
        out.push_str(&format!("[{label}] candidate_refusal {reason} {count}\n"));
    }
    let mut by_reason: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for reason in stats.refused_files.values() {
        *by_reason.entry(reason).or_default() += 1;
    }
    for (reason, count) in by_reason {
        out.push_str(&format!("[{label}] file_refusal {reason} {count}\n"));
    }
    out
}

fn edge_table(graph: &MayReachGraph) -> String {
    let mut out = String::new();
    for (caller, callee, info) in graph.edges() {
        for (start, end) in &info.sites {
            out.push_str(&format!(
                "{caller}\t{callee}\t{start}\t{end}\t{}\n",
                u8::from(info.union)
            ));
        }
    }
    out
}

fn python_files(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if !matches!(
                name.as_ref(),
                ".git" | ".venv" | "venv" | "node_modules" | "build" | "dist"
            ) && !name.starts_with('.')
            {
                python_files(root, &path, out);
            }
        } else if kind.is_file()
            && name.ends_with(".py")
            && let Ok(relative) = path.strip_prefix(root)
        {
            out.push(relative.to_path_buf());
        }
    }
}

#[test]
fn the_report_accounts_for_a_fixture_workspace() -> Check {
    let enforced = graph(&CHAIN);
    let text = summary("fixture", &enforced);
    ensure(
        text.contains("edges=") && text.contains("call_sites="),
        || text.clone(),
    )?;
    ensure(!edge_table(&enforced).is_empty(), || text)
}

/// Measurement, not a gate. `RIPR_MAY_REACH_CORPUS=<dir>` names a checked-out
/// Python project; `RIPR_MAY_REACH_REPORT=<prefix>` receives
/// `<prefix>.summary.txt` and the per-site `<prefix>.<mode>.edges.tsv` files
/// that a runtime call trace of the project's own tests is compared against.
///
/// ```text
/// RIPR_MAY_REACH_CORPUS=/path/to/click RIPR_MAY_REACH_REPORT=/tmp/click \
///   cargo test -p ripr --lib may_reach_corpus_report -- --ignored --nocapture
/// ```
#[test]
#[ignore = "measurement over an external corpus; set RIPR_MAY_REACH_CORPUS"]
fn may_reach_corpus_report() -> Check {
    let Ok(root) = std::env::var("RIPR_MAY_REACH_CORPUS") else {
        return Err("set RIPR_MAY_REACH_CORPUS to a checked-out Python project".to_string());
    };
    let root = PathBuf::from(root);
    let mut files = Vec::new();
    python_files(&root, &root, &mut files);
    files.sort();
    let mut sources = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(root.join(&file))
            .map_err(|err| format!("read {}: {err}", file.display()))?;
        sources.push((file, text));
    }
    let mut report = String::new();
    for (label, mode) in [
        ("enforce", GuardMode::Enforce),
        ("observe", GuardMode::Observe),
    ] {
        let graph = build(&sources, mode);
        report.push_str(&summary(label, &graph));
        if let Ok(prefix) = std::env::var("RIPR_MAY_REACH_REPORT") {
            std::fs::write(format!("{prefix}.{label}.edges.tsv"), edge_table(&graph))
                .map_err(|err| format!("write edges: {err}"))?;
        }
    }
    println!("{report}");
    if let Ok(prefix) = std::env::var("RIPR_MAY_REACH_REPORT") {
        std::fs::write(format!("{prefix}.summary.txt"), &report)
            .map_err(|err| format!("write summary: {err}"))?;
    }
    Ok(())
}
