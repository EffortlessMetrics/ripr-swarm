import pathlib,json,re,subprocess,collections,time
P=pathlib.Path('/workspace/scratch/profile-8258-native');D=P/'diagnostic';snap=[json.loads(l) for l in (D/'samples.jsonl').read_text().splitlines()];stacks=[json.loads(l) for l in (D/'stacks.jsonl').read_text().splitlines()];programs={};addresses=collections.defaultdict(set);refs=[]
def loads(path):
 if path not in programs:
  out=subprocess.check_output(['readelf','-lW',path],text=True);segments=[]
  for l in out.splitlines():
   v=l.split()
   if v and v[0]=='LOAD':segments.append({'offset':int(v[1],16),'vaddr':int(v[2],16),'size':int(v[4],16),'exec':'E' in v[6:-1]})
  programs[path]=segments
 return programs[path]
def frame(ip,position,maps):
 for line in maps.splitlines():
  v=line.split(maxsplit=5)
  if len(v)<6:continue
  lo,hi=[int(a,16) for a in v[0].split('-')]
  if not lo<=ip<hi:continue
  path=v[5];off=int(v[2],16)
  if not path.startswith('/') or not pathlib.Path(path).is_file():return None
  segments=[s for s in loads(path) if (s['offset']//4096*4096)<=off<((s['offset']+max(1,s['size'])+4095)//4096*4096) and s['exec']==('x' in v[1])]
  if not segments:return None
  s=segments[0];virtual_start=s['vaddr']//4096*4096+off-s['offset']//4096*4096;addr=ip-lo+virtual_start-(1 if position else 0);addresses[path].add(addr);return (path,addr)
for st in stacks:
 maps=snap[st['snapshot_index']]['maps'];row=[]
 for n,ip in enumerate(st['ips']):row.append(frame(ip,n,maps))
 refs.append(row)
lookup={};beg=time.monotonic()
for path,items in addresses.items():
 import bisect
 result=subprocess.run(['nm','-n','-S','-C','--defined-only',path],text=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=45,check=True)
 entries=[]
 for line in result.stdout.splitlines():
  v=line.split(maxsplit=3)
  if len(v)==4 and v[2] in ['t','T','w','W']:
   try: entries.append((int(v[0],16),int(v[1],16),v[3]))
   except ValueError: pass
 entries.sort(); starts=[r[0] for r in entries]
 for addr in items:
  i=bisect.bisect_right(starts,addr)-1
  name=entries[i][2] if i>=0 and addr<entries[i][0]+entries[i][1] else '??'
  lookup[(path,addr)]=[{'function':name,'location':'symbol table only; no line resolution'}]
print('symbolized',len(lookup),'PCs',len(addresses),'modules','seconds',time.monotonic()-beg,flush=True)
(P/'symbols.json').write_text(json.dumps([{'module':k[0],'address':hex(k[1]),'frames':v} for k,v in lookup.items()],indent=2)+'\n')
output=[];counts=collections.Counter();weight=collections.Counter();leaf=collections.Counter();anchors=collections.Counter();truncated=0;unresolved=0
for st,row in zip(stacks,refs):
 frames=[]
 for ref in row:
  if ref is None:frames.append({'function':'[unmapped]','location':'?'});continue
  frames.extend(lookup.get(ref,[{'function':'[unresolved]','location':'?'}]))
 names=[f['function'] for f in frames];text='\n'.join(names);bucket='other_or_unresolved'
 if any(t in text for t in ['store_file_facts','encode_file_facts','ripr::analysis::seam_cache::']):bucket='cache_store_encode_hash'
 elif any(t in text for t in ['rayon_core::sleep::Sleep::sleep','std::thread::functions::sleep','std::sync::mpmc::context::Context::wait_until']):bucket='endpoint_wait_transition_unattributed'
 elif 'load_file_facts' in text or 'decode_file_facts' in text:bucket='cache_load_decode'
 elif any(t in text for t in ['analysis::classify::','owner_pin_assertions','::has_escape','literal_json_import_visible','test_grip_evidence']):bucket='classification_owner_evidence'
 elif any(t in text for t in ['ra_ap_parser::','ra_ap_syntax::parsing','::parse_source_file','SourceFile::parse']):bucket='syntax_parse'
 elif any(t in text for t in ['extract_parser_probe_shapes','ProbeShapeNodes::collect']):bucket='probe_extraction'
 elif any(t in text for t in ['syntax::ra::summarize_file','syntax::ra::extract_parser_oracles','syntax::ra::collect_','syntax::ra::module_item_scopes']):bucket='other_file_facts_extraction'
 if st.get('error'):bucket='observer_error_or_empty'
 counts[bucket]+=1;weight[bucket]+=st['observed_cpu_delta_s'];leaf[names[0] if names else '[empty]']+=st['observed_cpu_delta_s'];truncated+=bool(st.get('truncated'));unresolved+=not any(n not in ['??','[unresolved]','[unmapped]'] for n in names)
 anchor=next((n for n in names if n.startswith('ripr::')),names[0] if names else '[empty]');anchors[anchor]+=st['observed_cpu_delta_s'];output.append({**st,'bucket':bucket,'frames':frames})
with (P/'symbolized-stacks.jsonl').open('w') as f:
 for r in output:f.write(json.dumps(r)+'\n')
total=sum(weight.values());result={'sample_count':len(stacks),'resolved_unique_pcs':len(lookup),'modules':len(addresses),'frame_limit':32,'truncated_stacks':truncated,'fully_unresolved_stacks':unresolved,'represented_cpu_delta_s':total,'buckets':[{'name':n,'samples':counts[n],'cpu_delta_weight_s':w,'percent_of_represented_cpu_deltas':100*w/total if total else None} for n,w in weight.most_common()],'top_first_ripr_frames':anchors.most_common(22),'top_leaf_frames':leaf.most_common(15),'phase_trace':[l for l in (D/'stderr.log').read_text().splitlines() if l.startswith('ripr_repo_exposure_latency')],'io_first':snap[0]['io'],'io_last':snap[-1]['io'],'symbolization_elapsed_s':time.monotonic()-beg,'limitations':['Symbol table function resolution only; no source lines or inline expansion.','CPU-delta-weighted endpoint-stack estimates, not exclusive instrumented timings.','32-frame truncation can hide owning ancestors; unresolved/other retained.','Cache serialization categorized only with cache store/encoder ancestry; serde alone is insufficient.']}
(P/'analysis.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2),flush=True)
