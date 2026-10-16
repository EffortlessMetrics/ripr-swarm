import datetime,hashlib,json,os,pathlib,resource,statistics,subprocess,time
base=pathlib.Path('/workspace/scratch/8169-measurements')
base.mkdir(exist_ok=True)
binary=pathlib.Path('/workspace/scratch/8169-proof/ripr').resolve()
version=subprocess.run([str(binary),'--version'],capture_output=True,text=True,check=True).stdout.strip()
assert '6a47f5bf094e5b010845b544f9464333a8b90d7d' in version,version
started=datetime.datetime.now(datetime.timezone.utc).isoformat()
rows=[]
def sha(data): return hashlib.sha256(data).hexdigest()
def run(args,env,stdout,stderr):
    with stdout.open('wb') as out,stderr.open('wb') as err:
        usage0=resource.getrusage(resource.RUSAGE_CHILDREN); wall0=time.perf_counter()
        completed=subprocess.run([str(binary)]+args,stdout=out,stderr=err,env=env,timeout=60)
        wall=time.perf_counter()-wall0; usage1=resource.getrusage(resource.RUSAGE_CHILDREN)
    if completed.returncode: raise RuntimeError((args,completed.returncode,stderr.read_text()[-3000:]))
    return wall,(usage1.ru_utime-usage0.ru_utime)+(usage1.ru_stime-usage0.ru_stime)
for size in [1,64]:
    root=base/f'input-{size}'; root.mkdir(exist_ok=False);(root/'src').mkdir()
    (root/'Cargo.toml').write_text('[package]\nname = "measurement"\nversion = "0.0.0"\nedition = "2024"\n[workspace]\n')
    (root/'ripr.toml').write_text('[analysis]\nmode = "ready"\n[languages]\nenabled = ["rust"]\n')
    (root/'src/lib.rs').write_text(''.join(f'pub mod owner{i};\n' for i in range(size)))
    diff=''
    for i in range(size):
        (root/f'src/owner{i}.rs').write_text('pub fn positive(value: i32) -> bool { value > 0 }\n#[cfg(test)] mod tests { #[test] fn notices_boundary() { assert!(super::positive(1)); assert!(!super::positive(0)); } }\n')
        diff+=f'--- a/src/owner{i}.rs\n+++ b/src/owner{i}.rs\n@@ -1 +1 @@\n-pub fn positive(value: i32) -> bool {{ value >= 0 }}\n+pub fn positive(value: i32) -> bool {{ value > 0 }}\n'
    patch=base/f'{size}.diff';patch.write_text(diff)
    args=['check','--root',str(root),'--diff',str(patch),'--quiet']
    for condition in ['cold','warm']:
        if condition=='warm':
            for variant in ['separate','fanout']:
                env=dict(os.environ,RIPR_CACHE_DIR=str(base/f'cache-{size}-{variant}'))
                run(args+['--format','human'],env,base/f'warmup-{size}-{variant}.stdout',base/f'warmup-{size}-{variant}.stderr')
        for rep in range(5):
            pair={}
            for variant in (['separate','fanout'] if rep%2==0 else ['fanout','separate']):
                tag=f'{size}-{condition}-{rep}-{variant}'
                cache=base/f'cache-{size}-{variant}' if condition=='warm' else base/f'cache-{tag}'
                env=dict(os.environ,RIPR_CACHE_DIR=str(cache))
                wall=cpu=0.;outputs={};sets=base/f'set-{tag}'
                if variant=='separate':
                    for fmt,name in [('badge-json','badge.json'),('json','detail.json'),('human','human.txt')]:
                        out=base/f'{tag}-{name}';err=base/f'{tag}-{name}.stderr'
                        w,c=run(args+['--format',fmt],env,out,err);wall+=w;cpu+=c
                        outputs[name]=out.read_bytes()
                    manifest_bytes=0
                else:
                    w,c=run(args+['--output-dir',str(sets)],env,base/f'{tag}.stdout',base/f'{tag}.stderr');wall+=w;cpu+=c
                    manifest=json.loads((sets/'manifest.json').read_text())
                    assert manifest['complete_set'] and manifest['command_success']
                    for name in ['badge.json','detail.json','human.txt']: outputs[name]=(sets/name).read_bytes()
                    manifest_bytes=(sets/'manifest.json').stat().st_size
                row=dict(size=size,condition=condition,repetition=rep,variant=variant,wall_seconds=wall,cpu_seconds=cpu,member_bytes=sum(map(len,outputs.values())),manifest_bytes=manifest_bytes,output_sha256={name:sha(data) for name,data in outputs.items()},diff_sha256=sha(patch.read_bytes()),config_sha256=sha((root/'ripr.toml').read_bytes()))
                rows.append(row);pair[variant]=outputs
            assert pair['separate']==pair['fanout'],(size,condition,rep,'native byte divergence')
    print('completed workload',size,flush=True)
summary=[]
for size in [1,64]:
 for condition in ['cold','warm']:
  group={variant:[r for r in rows if r['size']==size and r['condition']==condition and r['variant']==variant] for variant in ['separate','fanout']}
  summary.append(dict(size=size,condition=condition,**{variant:dict(wall_median=statistics.median(r['wall_seconds'] for r in rs),cpu_median=statistics.median(r['cpu_seconds'] for r in rs),member_bytes=rs[0]['member_bytes'],manifest_bytes=rs[0]['manifest_bytes']) for variant,rs in group.items()}))
receipt=dict(head='6a47f5bf094e5b010845b544f9464333a8b90d7d',tree='2a15b485aa9684ff9ea05159e429641433dd550b',base='b772d9e13cafcdc84a19ea77d9b90fbc6cb91ecc',started=started,finished=datetime.datetime.now(datetime.timezone.utc).isoformat(),binary=str(binary),binary_sha256=sha(binary.read_bytes()),version=version,method='Linux resource.RUSAGE_CHILDREN user+system CPU; perf_counter wall; sequential native processes with stdout/stderr redirected to files; 5 alternating AB/BA pairs per workload/condition. cold = new analyzer cache per variant per pair; warm prewarms each variant cache with native human output, then reuses that cache across five repetitions. Separate cold groups reuse their initially empty cache for the second and third standalone processes, matching the consumer pattern. OS caches not controlled; dev profile. Common version probe excluded from both variants. Exact member byte equality every pair. No 3x assumption.',environment={key:os.environ.get(key) for key in ['PATH','CARGO_HOME','RUSTUP_HOME','RUSTUP_TOOLCHAIN','CARGO_INCREMENTAL','CARGO_BUILD_JOBS']},rows=rows,summary=summary)
(base/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(json.dumps(summary,indent=2))
