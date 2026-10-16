import pathlib,json,statistics
p=pathlib.Path('/workspace/scratch/probe-shape-reuse/corrected-benchmark');b=json.loads((p/'benchmark.json').read_text());assert len(b['records'])==6
result={'scope':'comparison-only: debug binaries, Linux, one completed fixed input; no full-repository timeout/qualification inference','measurement_scope':'wall: full command; CPU/RSS: Linux wait4 root process (including its worker threads), not a sampled process-tree peak','input_bytes':b['input_bytes'],'input_hashes':b['input_hashes'],'workload_functions':b['workload_functions'],'functions':b['functions'],'pairs':3,'deadline_s_per_run':30,'all_completed':all(r['exit']==0 and not r['censored'] and not r['remaining_owned_processes'] for r in b['records']),'all_stdout_byte_identical':len({r['stdout_sha256'] for r in b['records']})==1,'samples':{}}
for v in ['baseline','candidate']:
 rows=[r for r in b['records'] if r['label'].endswith(v)];assert len(rows)==3
 wall=[r['elapsed_s'] for r in rows];cpu=[r['cpu_s'] for r in rows];rss=[r['maxrss_kib'] for r in rows]
 result['samples'][v]={'wall_s':wall,'wall_median_s':statistics.median(wall),'cpu_s':cpu,'cpu_median_s':statistics.median(cpu),'maxrss_kib':rss,'peak_maxrss_kib':max(rss),'binary_sha256':rows[0]['binary_sha256']}
result['median_wall_reduction_percent']=100*(1-result['samples']['candidate']['wall_median_s']/result['samples']['baseline']['wall_median_s'])
(p/'comparison.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
