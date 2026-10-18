from pathlib import Path
import json,shutil
R=Path('/workspace/macro-site-parity');P=Path('/workspace/scratch/8265-reconstruction');F=R/'fixtures/macro_site_scoped_rest_limit';assert not F.exists();src=P/'controls/scoped-public-only'
for p in src.rglob('*'):
 if p.is_file() and p.name!='change.diff':
  q=F/'input'/p.relative_to(src);q.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(p,q)
(F/'diff.patch').write_bytes((src/'change.diff').read_bytes());(F/'expected').mkdir();(F/'expected/human-full.txt').write_text('');(F/'SPEC.md').write_text('''# Fixture: macro_site_scoped_rest_limit

Spec: RIPR-SPEC-0240

Issue: #8265

## Given

A foreign glob makes assertion bindings uncertain. An unrelated inline module
in the integration-test file defines `assert_eq`, outside the real assertion's
scope. The likely-file pass saturates on the glob and defers that definition.

## When

`cargo xtask fixtures macro_site_scoped_rest_limit` analyzes the changed bound.

## Then

The nonempty findings remain `static_unknown`, retain
`rust_assertion_context_unresolved` and `gap_evidence_unresolved`, and name the
unscoped foreign glob. They have no canonical gap IDs.

## Must Not

- Select the unrelated scoped definition as the workspace refusal.
- Promote analyzer uncertainty to an actionable gap or claim test adequacy.
- Accept an empty or all-exposed re-blessing.
''')
p=R/'fixtures/evidence-promotion-honesty-corpus/corpus.json';s=p.read_text();assert 'rust_macro_site_scoped_rest_limit' not in s;item={'id':'rust_macro_site_scoped_rest_limit','language':'rust','tier':'pure','source_fixture':'fixtures/macro_site_scoped_rest_limit','vector':'scoped_rest_definition_cannot_promote_binding_uncertainty_to_a_gap','assertions':[{'type':'must_not_report_clean'},{'type':'expected_class','class':'static_unknown'},{'type':'must_not_promote'}]};pos=s.rindex('\n  ]');s=s[:pos]+',\n'+'\n'.join('    '+line for line in json.dumps(item,indent=2).splitlines())+s[pos:];p.write_text(s);json.loads(s)
p=R/'.ripr/traceability.toml';s=p.read_text();a=s.index('id = "RIPR-SPEC-0240"');b=s.index('\n[[behavior]]',a);section=s[a:b];tests=['reconstructed_parity_inline_scope','reconstructed_parity_function_scope','reconstructed_parity_singleton_arguments','reconstructed_parity_unparsed_prefilter','reconstructed_parity_newline_private_first','reconstructed_parity_same_line_private_first','reconstructed_parity_same_line_public_first','reconstructed_parity_rebinding_and_root_control'];section=section.replace('tests = [','tests = [\n'+'\n'.join('  "crates/ripr/src/analysis/classify/owner_pin/tests/retention_parity.rs::'+name+'",' for name in tests),1).replace('fixtures = [','fixtures = [\n  "fixtures/macro_site_scoped_rest_limit",',1).replace('outputs = [','outputs = [\n  "fixtures/macro_site_scoped_rest_limit/expected/check.json",',1);p.write_text(s[:a]+section+s[b:]);p=R/'changelog.d/8265-macro-site-parity.md';assert not p.exists();p.write_text('<!-- section: Fixed -->\n- Rust macro-binding disclosures preserve per-name argument context and\n  same-file order, and exclude unrelated scoped definitions during deferred\n  completion, preventing false actionable gaps from analyzer uncertainty (#8265).\n')
