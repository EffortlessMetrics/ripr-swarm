use super::super::gap_decision_ledger::{self, GapRecord};
use super::model::{BaselineIndex, CalibrationEvidence, CalibrationIndex, GateEvaluateInput};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn read_labels(input: &GateEvaluateInput, warnings: &mut Vec<String>) -> Result<Vec<String>, String> {
    let mut labels = input.labels.iter().filter(|l| !l.trim().is_empty()).cloned().collect::<BTreeSet<_>>();
    if let Some(path) = &input.labels_json {
        let resolved = resolve_root_path(&input.root, path);
        match read_json_value_with_display(&resolved, path) {
            Ok(value) => for label in labels_from_value(&value) { labels.insert(label); },
            Err(error) => warnings.push(format!("optional labels_json {} is unavailable: {error}", display_path(path))),
        }
    }
    Ok(labels.into_iter().collect())
}

fn labels_from_value(value: &Value) -> Vec<String> { if let Some(values)=value.as_array(){return values.iter().filter_map(Value::as_str).map(ToOwned::to_owned).collect();} value.get("labels").and_then(Value::as_array).map(|v| v.iter().filter_map(Value::as_str).map(ToOwned::to_owned).collect()).unwrap_or_default() }

pub(super) fn warn_for_optional_json(root:&Path,path:Option<&PathBuf>,name:&str,warnings:&mut Vec<String>){let Some(path)=path else{return;}; if let Err(error)=read_json_value_with_display(&resolve_root_path(root,path),path){warnings.push(format!("optional {name} {} is unavailable: {error}",display_path(path)));}}

pub(super) fn read_gap_ledger(input:&GateEvaluateInput, config_errors:&mut Vec<String>)->Option<Vec<GapRecord>>{ let path=input.gap_ledger.as_ref()?; let resolved=resolve_root_path(&input.root,path); let text=match fs::read_to_string(&resolved){Ok(t)=>t,Err(error)=>{config_errors.push(format!("required gap decision ledger input {} is invalid: read failed: {error}",display_path(path))); return Some(Vec::new());}}; match gap_decision_ledger::parse_gap_records_json(&text){Ok(r)=>Some(r),Err(error)=>{config_errors.push(format!("required gap decision ledger input {} is invalid: {error}",display_path(path))); Some(Vec::new())}} }

pub(super) fn read_recommendation_calibration(input:&GateEvaluateInput,warnings:&mut Vec<String>)->CalibrationIndex{ let mut index=CalibrationIndex::default(); let Some(path)=&input.recommendation_calibration else{return index;}; let resolved=resolve_root_path(&input.root,path); let value=match read_json_value_with_display(&resolved,path){Ok(v)=>v,Err(error)=>{warnings.push(format!("optional recommendation_calibration {} is unavailable: {error}",display_path(path))); return index;}}; for item in value.get("recommendations").and_then(Value::as_array).into_iter().flatten(){ let outcome=item.pointer("/calibration/outcome").and_then(Value::as_str); let evidence=CalibrationEvidence{available:true,outcome:super::string_field(item.pointer("/calibration/outcome")),confidence_effect:super::recommendation_confidence_effect(outcome).to_string()}; if let Some(id)=item.get("id").and_then(Value::as_str){index.by_source_id.insert(id.to_string(),evidence.clone());} if let Some(seam_id)=item.get("seam_id").and_then(Value::as_str){index.by_seam_id.insert(seam_id.to_string(),evidence);} } index }

pub(super) fn read_mutation_calibration(input:&GateEvaluateInput,warnings:&mut Vec<String>)->CalibrationIndex{ let mut index=CalibrationIndex::default(); let Some(path)=&input.mutation_calibration else{return index;}; let resolved=resolve_root_path(&input.root,path); let value=match read_json_value_with_display(&resolved,path){Ok(v)=>v,Err(error)=>{warnings.push(format!("optional mutation_calibration {} is unavailable: {error}",display_path(path))); return index;}}; for item in value.get("matches").and_then(Value::as_array).into_iter().flatten(){ let seam_id=item.pointer("/static/seam_id").and_then(Value::as_str).or_else(||item.pointer("/runtime/seam_id").and_then(Value::as_str)); let Some(seam_id)=seam_id else{continue;}; let outcome=item.pointer("/runtime/runtime_outcome").and_then(Value::as_str).or_else(||item.pointer("/runtime/outcome").and_then(Value::as_str)); index.by_seam_id.insert(seam_id.to_string(),CalibrationEvidence{available:true,outcome:outcome.map(ToOwned::to_owned),confidence_effect:super::mutation_confidence_effect(outcome).to_string()}); }
if !value.get("ambiguous_file_line_matches").and_then(Value::as_array).map(|items| items.is_empty()).unwrap_or(true){warnings.push(format!("mutation_calibration {} contains ambiguous file/line matches; those records do not raise gate confidence",display_path(path)));} index }

pub(super) fn read_baseline(input:&GateEvaluateInput,warnings:&mut Vec<String>,config_errors:&mut Vec<String>)->BaselineIndex{ if input.mode.requires_baseline()&&input.baseline.is_none(){config_errors.push(format!("{} mode requires an explicit --baseline artifact",input.mode.as_str())); return BaselineIndex::default();} let Some(path)=&input.baseline else{return BaselineIndex::default();}; let resolved=resolve_root_path(&input.root,path); match read_json_value_with_display(&resolved,path){Ok(v)=>super::baseline_index_from_value(&v),Err(error) if input.mode.requires_baseline()=>{config_errors.push(format!("required baseline {} is invalid: {error}",display_path(path))); BaselineIndex::default()},Err(error)=>{warnings.push(format!("optional baseline {} is unavailable: {error}",display_path(path))); BaselineIndex::default()}} }

pub(super) fn read_json_value_with_display(path:&Path,display:&Path)->Result<Value,String>{ let display=display_path(display); let text=fs::read_to_string(path).map_err(|err| if err.kind()==std::io::ErrorKind::NotFound {format!("read {display} failed: not found")} else {format!("read {display} failed: {err}")})?; serde_json::from_str(&text).map_err(|err| format!("parse {display} failed: {err}")) }

pub(super) fn resolve_root_path(root:&Path,path:&Path)->PathBuf{ if path.is_absolute(){path.to_path_buf()} else {root.join(path)}}
pub(super) fn display_path(path:&Path)->String{ let value=path.display().to_string().replace('\\',"/"); if value.is_empty(){".".to_string()} else {value.strip_prefix("./").unwrap_or(&value).to_string()} }
