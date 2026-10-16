import os, pathlib, shutil, subprocess, json, hashlib, datetime
src=pathlib.Path('/workspace/review-8229-integration'); evidence=pathlib.Path('/workspace/review-8229-integration-evidence'); binary=pathlib.Path('/workspace/review-8229-target/debug/ripr')
records=[]
def invoke(root,label,fmt,budget):
    argv=[str(binary),'check','--root',str(root),'--diff',str(root/'change.patch'),'--mode','fast','--format',fmt,'--quiet']
    env=os.environ.copy(); env['RIPR_PARTIAL_DIFF_FILE_BUDGET']=str(budget)
    p=subprocess.run(argv,env=env,capture_output=True,timeout=60)
    prefix=evidence/(label+'-'+fmt); prefix.with_suffix('.stdout').write_bytes(p.stdout);prefix.with_suffix('.stderr').write_bytes(p.stderr)
    records.append(dict(label=label,argv=argv,budget=budget,config=(root/'ripr.toml').read_text(),patch=(root/'change.patch').read_text(),exit=p.returncode,stdout_sha256=hashlib.sha256(p.stdout).hexdigest(),stderr_sha256=hashlib.sha256(p.stderr).hexdigest()))
    return p,json.loads(p.stdout) if fmt=='json' else p.stdout.decode()
for lang,ext,display in [('python','py','Python'),('typescript','ts','TypeScript'),('javascript','js','JavaScript')]:
    root=evidence/('case-'+lang);root.mkdir(exist_ok=True)
    for part in ['src','tests']:(root/part).mkdir(exist_ok=True)
    for f in ['Cargo.toml','src/lib.rs','tests/pricing.rs']:shutil.copy(src/'fixtures/boundary_gap/input'/f,root/f)
    fixturelang='typescript' if lang=='javascript' else lang
    fixtureext='ts' if lang=='javascript' else ext
    configlang='typescript' if lang=='javascript' else lang
    preview=src/'fixtures'/f'{fixturelang}_boundary_gap'
    content=(preview/f'input/src/discount.{fixtureext}').read_text()
    if lang=='javascript': content=content.replace(': number','')
    (root/f'src/discount.{ext}').write_text(content)
    (root/f'src/extra.{ext}').write_text(content.replace('apply_discount','extra_discount').replace('applyDiscount','extraDiscount'))
    rustpatch=(src/'fixtures/boundary_gap/diff.patch').read_text(); patch=(preview/'diff.patch').read_text().replace(f'discount.{fixtureext}',f'discount.{ext}')
    if lang=='javascript': patch=patch.replace(': number','')
    combined=rustpatch+patch+patch.replace(f'discount.{ext}',f'extra.{ext}').replace('apply_discount','extra_discount').replace('applyDiscount','extraDiscount')
    for enabled,budget in [(True,1),(True,2),(True,3),(False,1),(False,3)]:
        (root/'ripr.toml').write_text('[languages]\nenabled = '+json.dumps(['rust',configlang] if enabled else ['rust'])+'\n')
        (root/'change.patch').write_text(combined)
        label=f'{lang}-enabled{enabled}-budget{budget}'
        jp,j=invoke(root,label,'json',budget); hp,h=invoke(root,label,'human',budget)
        expected=budget-1 if enabled else 0
        assert jp.returncode==hp.returncode==0,label
        if not (enabled and budget==3): assert 'analysis incomplete; partial_with_limitations' in h,label
        assert j['analysis_outcome']['analysis_complete']==(enabled and budget==3),label
        assert j['analysis_outcome']['outcome']['kind']==('complete_with_findings' if enabled and budget==3 else 'partial_with_limitations'),label
        assert j['analysis_outcome']['outcome']['counts']['changed_file_count']==3,label
        files={f['probe']['file'].removeprefix('./') for f in j['findings']}
        assert 'src/lib.rs' in files,label
        assert len(files & {f'src/discount.{ext}',f'src/extra.{ext}'})==expected,label
        advisories=j.get('preview_languages',[])
        if enabled and budget==1:
            assert not advisories,label
            assert 'analyzed under preview support' not in h,label
        else:
            a=advisories[0];assert a['enabled']==a['analyzed']==enabled,label
            assert a['file_count']==(expected if enabled else 2),label
            assert a['sample_paths']==([f'src/discount.{ext}'] if enabled and expected==1 else [f'src/discount.{ext}',f'src/extra.{ext}']),label
            if enabled: assert f'{expected} {display} '+('file' if expected==1 else 'files')+' analyzed under preview support' in h,label
            else:assert 'files were not analyzed' in h and 'To enable, add to ripr.toml' in h,label
        if enabled and budget<3:
            scope=j['analysis_scope'];assert scope['selected_files']==(['src/lib.rs'] if budget==1 else ['src/lib.rs',f'src/discount.{ext}']),label
            assert scope['downstream_consumable'] is False and scope['gate_eligibility']=='ineligible',label
            assert 'run state limited_partial_scope' in h and 'gate_eligibility: ineligible' in h,label
        records[-2]['observed']=dict(kind=j['analysis_outcome']['outcome']['kind'],complete=j['analysis_outcome']['analysis_complete'],advisories=advisories,files=sorted(files),scope=j.get('analysis_scope'))
    # A malformed second hunk must preserve the usable Rust subject and native unsupported exit2.
    (root/'ripr.toml').write_text('[languages]\nenabled = ["rust", "'+configlang+'"]\n')
    (root/'change.patch').write_text(rustpatch+f'diff --git a/src/discount.{ext} b/src/discount.{ext}\n--- a/src/discount.{ext}\n+++ b/src/discount.{ext}\n@@ -1,4 +1,4 @@\n')
    label=f'{lang}-unsupported-mixed'
    jp,j=invoke(root,label,'json',3);hp,h=invoke(root,label,'human',3)
    assert jp.returncode==hp.returncode==2,label
    assert j['analysis_outcome']['outcome']['kind']=='unsupported_input' and j['analysis_outcome']['analysis_complete'] is False,label
    assert j['findings'] and 'unsupported_input' in h,label
    records[-2]['observed']=dict(kind=j['analysis_outcome']['outcome']['kind'],complete=False,findings=len(j['findings']))
receipt=dict(head='717e16559445806c75e0b13a483745da80504af9',reviewed_candidate='67dc5af9a15590fc8476d09217232831a0009952',timestamp=datetime.datetime.now(datetime.timezone.utc).isoformat(),binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),invocations=records,all_assertions_passed=True)
(evidence/'native-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(f'PASS: {len(records)} independent native invocations; exact JSON documents and human projections retained')
