use std::{env, fs, path::PathBuf, process::{Command, Stdio}, time::Instant, io::Write};

fn module(i: usize, asynchronous: bool) -> String {
    let qualifier = if asynchronous { "async " } else { "" };
    let assertion = if asynchronous {
        "let _future = read_read_load_as_object(); assert_eq!(7, 7);"
    } else { "assert_eq!(read_read_load_as_object(), Some(7));" };
    format!(r#"mod unit_{i} {{
#[derive(Clone, Copy)]
pub(crate) enum JsonValue {{ Object(u32), Null }}
impl JsonValue {{
    pub(crate) fn as_object(&self) -> Option<u32> {{
        match self {{ Self::Object(value) => Some(*value), _ => None }}
    }}
}}
{qualifier}fn load_as_object() -> Option<u32> {{ JsonValue::Object(7).as_object() }}
{qualifier}fn read_load_as_object() -> Option<u32> {{ {} }}
{qualifier}fn read_read_load_as_object() -> Option<u32> {{ {} }}
#[cfg(test)] mod tests {{ use super::*;
#[test] fn subject() {{ {assertion} }}
}}
}}
"#, if asynchronous { "load_as_object().await" } else { "load_as_object()" },
        if asynchronous { "read_load_as_object().await" } else { "read_load_as_object()" })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    let home = PathBuf::from(&args[2]);
    if args[1] == "generate" {
        for kind in ["sync", "async", "direct"] {
            for n in [4, 16, 64] {
                for split in [false, true] {
                    if split && n != 64 { continue; }
                    let name = format!("{kind}-{n}-{}", if split {"split"} else {"single"});
                    let root = home.join("workloads").join(&name);
                    fs::create_dir_all(root.join("src"))?;
                    fs::write(root.join("Cargo.toml"), format!("[package]\nname = \"bench-{}\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n", name))?;
                    let mut lib = String::new();
                    for i in 0..n {
                        let body = if kind == "direct" {
                            format!("mod unit_{i} {{ pub(crate) fn calculate(x: u32) -> u32 {{ x + 1 }}\n#[cfg(test)] mod tests {{ use super::*; #[test] fn subject() {{ assert_eq!(calculate(6), 7); }} }} }}\n")
                        } else { module(i, kind == "async") };
                        if split {
                            lib.push_str(&format!("mod file_{i};\n"));
                            fs::write(root.join(format!("src/file_{i}.rs")), body)?;
                        } else { lib.push_str(&body); }
                    }
                    fs::write(root.join("src/lib.rs"), lib)?;
                    println!("{name}");
                }
            }
        }
        return Ok(());
    }
    let count: usize = args[3].parse()?;
    let label = env::var("BENCH_RUN_LABEL").unwrap_or_else(|_| "main".to_string());
    let workload_filter = args.get(4).map(String::as_str).unwrap_or("");
    let mut workloads: Vec<_> = fs::read_dir(home.join("workloads"))?.map(|e| e.unwrap().path()).collect();
    workloads.sort();
    let raw = home.join("evidence/raw");
    fs::create_dir_all(&raw)?;
    let mut table = fs::OpenOptions::new().create(true).append(true).open(home.join("evidence/measurements.tsv"))?;
    for root in workloads {
        let workload = root.file_name().unwrap().to_str().unwrap();
        if !workload.contains(workload_filter) { continue; }
        for (mode, format) in [("full", "repo-exposure-json"), ("compact", "repo-exposure-summary-json")] {
            for round in 0..count {
                let order = if round % 2 == 0 { ["base", "candidate"] } else { ["candidate", "base"] };
                for variant in order {
                    let cache = home.join(format!("caches/{label}-{workload}-{mode}-{round}-{variant}"));
                    if cache.exists() { return Err(format!("cache already exists: {}", cache.display()).into()); }
                    for path in ["cold", "warm"] {
                        let id = format!("{label}-{workload}-{mode}-{round}-{variant}-{path}");
                        let stdout = fs::File::create(raw.join(format!("{id}.json")))?;
                        let stderr = fs::File::create(raw.join(format!("{id}.stderr")))?;
                        let binary = home.join(format!("bin/{variant}"));
                        let start = Instant::now();
                        let status = Command::new("timeout").args(["120s", binary.to_str().unwrap(), "check", "--root", root.to_str().unwrap(), "--format", format])
                            .env("RIPR_CACHE_DIR", &cache)
                            .env("RIPR_REPO_EXPOSURE_LATENCY_TRACE", "1")
                            .env("RAYON_NUM_THREADS", "4")
                            .stdout(Stdio::from(stdout)).stderr(Stdio::from(stderr)).status()?;
                        let seconds = start.elapsed().as_secs_f64();
                        writeln!(table, "{label}\t{workload}\t{mode}\t{round}\t{variant}\t{path}\t{seconds:.9}\t{}\t{}", status.code().unwrap_or(-1), fs::metadata(raw.join(format!("{id}.json")))?.len())?;
                        table.flush()?;
                        println!("{id} {seconds:.4}s {status}");
                        if !status.success() { return Err(format!("run failed; retained {id}").into()); }
                    }
                }
            }
        }
    }
    Ok(())
}
