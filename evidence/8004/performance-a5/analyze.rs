use std::{collections::BTreeMap, env, fs, path::Path};
use serde_json::{Value, json};

fn median(v: &[f64]) -> f64 {
    let mut v = v.to_vec(); v.sort_by(f64::total_cmp);
    if v.len() % 2 == 1 { v[v.len()/2] } else { (v[v.len()/2-1]+v[v.len()/2])/2.0 }
}
fn describe(v: &[f64]) -> Value {
    let m=median(v);
    json!({"n":v.len(),"median_s":m,"mad_s":median(&v.iter().map(|x|(x-m).abs()).collect::<Vec<_>>()),"min_s":v.iter().copied().fold(f64::INFINITY,f64::min),"max_s":v.iter().copied().fold(0.0,f64::max)})
}
fn differences(a: &Value,b: &Value,p: &str,out: &mut Vec<Value>) {
    if a==b || out.len()>=200 {return}
    match (a,b) {
        (Value::Object(a),Value::Object(b)) => {
            let keys:std::collections::BTreeSet<_>=a.keys().chain(b.keys()).collect();
            for k in keys { differences(a.get(k).unwrap_or(&Value::Null),b.get(k).unwrap_or(&Value::Null),&format!("{p}/{k}"),out); }
        },
        (Value::Array(a),Value::Array(b)) if a.len()==b.len() => {
            for (i,(a,b)) in a.iter().zip(b).enumerate() {differences(a,b,&format!("{p}/{i}"),out)}
        },
        _=>out.push(json!({"pointer":p,"base":a,"candidate":b}))
    }
}
fn main()->Result<(),Box<dyn std::error::Error>> {
    let args:Vec<_>=env::args().collect(); let home=Path::new(&args[1]);
    let text=fs::read_to_string(home.join("evidence/measurements.tsv"))?;
    let mut groups:BTreeMap<(String,String,String),BTreeMap<usize,BTreeMap<String,f64>>>=BTreeMap::new();
    let mut canonical:BTreeMap<(String,String,String),Value>=BTreeMap::new();
    let mut parity_failures=Vec::new(); let mut traces=BTreeMap::<String,usize>::new();
    let mut checked=0usize;
    for line in text.lines() {
        let f:Vec<_>=line.split('\t').collect();
        if f[0]!="main" {continue}
        let (workload,mode,round,variant,path)=(f[1],f[2],f[3].parse::<usize>()?,f[4],f[5]);
        groups.entry((workload.into(),mode.into(),path.into())).or_default().entry(round).or_default().insert(variant.into(),f[6].parse()?);
        if f[7]!="0" {return Err(format!("nonzero process: {line}").into())}
        let id=format!("main-{workload}-{mode}-{round}-{variant}-{path}");
        let mut value:Value=serde_json::from_slice(&fs::read(home.join(format!("evidence/raw/{id}.json")))?)?;
        if value["metrics"].is_null() {return Err(format!("no metrics {id}").into())}
        let total=value["metrics"][if mode=="full" {"seams_total"}else{"raw_seams"}].as_u64().ok_or("missing total")?;
        if total==0 {return Err(format!("zero workload {id}").into())}
        if mode=="full" && value["seams"].as_array().ok_or("missing seams")?.len() as u64 != total {return Err(format!("seam count mismatch {id}").into())}
        value.as_object_mut().ok_or("not object")?.remove("artifact");
        let key=(workload.into(),mode.into(),variant.into());
        if let Some(prior)=canonical.get(&key) {
            if prior!=&value {let mut diff=Vec::new(); differences(prior,&value,"",&mut diff); parity_failures.push(json!({"sample":id,"differences_first_200":diff}));}
        } else {canonical.insert(key,value);}
        let stderr=fs::read_to_string(home.join(format!("evidence/raw/{id}.stderr")))?;
        let statuses:Vec<_>=stderr.lines().filter(|l|l.starts_with("ripr_repo_exposure_latency phase=total ")).collect();
        if statuses.is_empty() {return Err(format!("missing latency trace {id}").into())}
        for s in &statuses { *traces.entry(format!("{mode}/{path}/{}",s.split("status=").nth(1).unwrap_or("missing").split_whitespace().next().unwrap_or("missing"))).or_default()+=1; }
        if path=="warm" && !statuses.iter().any(|s|s.contains("cache_hit")) {parity_failures.push(json!({"sample":id,"failure":"warm total was not a cache hit","total_trace":statuses}));}
        if path=="cold" && statuses.iter().any(|s|s.contains("cache_hit")) {parity_failures.push(json!({"sample":id,"failure":"cold total was a cache hit","total_trace":statuses}));}
        if stderr.contains("lexical fallback") || stderr.contains("lexical_fallback") {parity_failures.push(json!({"sample":id,"failure":"possible lexical fallback; inspect stderr"}));}
        checked+=1;
    }
    let mut statistics=Vec::new(); let mut table=String::from("workload\tmode\tpath\tn\tbase_median_s\tcandidate_median_s\tmedian_paired_ratio\tci95_lo\tci95_hi\tmedian_delta_s\tmaterial_regression\n");
    for ((workload,mode,path),rounds) in groups {
        let mut a=Vec::new();let mut b=Vec::new();
        for (_,r) in rounds {a.push(*r.get("base").ok_or("missing paired base")?);b.push(*r.get("candidate").ok_or("missing paired candidate")?)}
        let ratios:Vec<_>=a.iter().zip(&b).map(|(a,b)|b/a).collect();
        let deltas:Vec<_>=a.iter().zip(&b).map(|(a,b)|b-a).collect();
        let mut seed=8004u64; let mut boots=Vec::with_capacity(20000);
        for _ in 0..20000 {let v:Vec<_>=(0..ratios.len()).map(|_|{seed^=seed<<13;seed^=seed>>7;seed^=seed<<17;ratios[seed as usize%ratios.len()]}).collect();boots.push(median(&v));}
        boots.sort_by(f64::total_cmp);let ci=[boots[500],boots[19499]];
        let material=ci[0]>1.10 && median(&deltas)>if path=="cold"{0.020}else{0.005};
        table.push_str(&format!("{workload}\t{mode}\t{path}\t{}\t{:.9}\t{:.9}\t{:.6}\t{:.6}\t{:.6}\t{:.9}\t{material}\n",a.len(),median(&a),median(&b),median(&ratios),ci[0],ci[1],median(&deltas)));
        statistics.push(json!({"workload":workload,"mode":mode,"path":path,"base":describe(&a),"candidate":describe(&b),"median_paired_ratio":median(&ratios),"paired_ratio_ci95":ci,"median_paired_delta_s":median(&deltas),"paired_ratios":ratios,"material_regression":material}));
    }
    let workloads:std::collections::BTreeSet<_>=canonical.keys().map(|(w,_,_)|w.clone()).collect();
    let mut outputs=Vec::new();
    for workload in workloads {
        for mode in ["full","compact"] {
            let a=&canonical[&(workload.clone(),mode.into(),"base".into())];let b=&canonical[&(workload.clone(),mode.into(),"candidate".into())];
            let mut diff=Vec::new();differences(a,b,"",&mut diff);
            outputs.push(json!({"workload":workload,"mode":mode,"base_metrics":a["metrics"],"candidate_metrics":b["metrics"],"normalized_equal":a==b,"first_200_differences":diff}));
        }
        for variant in ["base","candidate"] {
            let full=&canonical[&(workload.clone(),"full".into(),variant.into())]["metrics"];
            let compact=&canonical[&(workload.clone(),"compact".into(),variant.into())]["metrics"];
            let mut grip=full.as_object().ok_or("bad metrics")?.clone();grip.remove("seams_total");grip.remove("headline_eligible");
            let equal=full["seams_total"]==compact["raw_seams"] && full["headline_eligible"]==compact["headline_eligible_seams"] && Value::Object(grip)==compact["grip_class"];
            outputs.push(json!({"workload":workload,"variant":variant,"full_compact_inventory_and_grip_parity":equal}));
        }
    }
    fs::write(home.join("evidence/statistics.json"),serde_json::to_vec_pretty(&statistics)?)?;
    fs::write(home.join("evidence/statistics.tsv"),table)?;
    fs::write(home.join("evidence/output-controls.json"),serde_json::to_vec_pretty(&json!({"samples_checked":checked,"normalization":"remove only top-level artifact from reports; preserve all remaining fields and array order","within_variant_failures":parity_failures,"total_trace_status_counts":traces,"outputs":outputs}))?)?;
    println!("samples_checked={checked}, within_variant_failures={}",parity_failures.len());
    Ok(())
}
