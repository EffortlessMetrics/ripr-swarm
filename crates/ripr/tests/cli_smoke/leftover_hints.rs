// #11570 items 2, 3 and 8: recovery hints and plain help wording.
use super::{run_ripr, unique_temp_workspace};

#[test]
fn agent_card_and_packet_without_an_id_say_where_to_get_one()
-> Result<(), Box<dyn std::error::Error>> {
    for args in [["agent", "card"], ["agent", "packet"]] {
        let output = run_ripr(&args);
        assert_eq!(output.status.code(), Some(2), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("Get an id from `ripr pilot --root"),
            "{args:?} must say where an id comes from: {stderr}"
        );
    }
    Ok(())
}

#[test]
fn check_diff_with_a_revision_name_points_at_base() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_workspace("diff-revision-hint");
    std::fs::create_dir_all(&root)?;
    let root_arg = root.display().to_string();
    let output = run_ripr(&["check", "--root", &root_arg, "--diff", "HEAD"]);
    let _ = std::fs::remove_dir_all(&root);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("pass the path of an existing unified diff file")
            && stderr.contains("use `--base REV`"),
        "a revision passed to --diff must point at --base: {stderr}"
    );
    Ok(())
}

#[test]
fn top_level_help_task_map_avoids_the_internal_word_seam() {
    for args in [["--help"].as_slice(), ["help", "--all"].as_slice()] {
        let output = run_ripr(args);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let task_map: String = stdout
            .lines()
            .filter(|line| line.trim_start().starts_with("Repair a "))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(task_map.contains("Repair a Rust gap"), "{task_map}");
        assert!(!task_map.contains("seam"), "{task_map}");
    }
}
