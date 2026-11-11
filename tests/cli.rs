use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;

fn fixture() -> &'static str {
    concat!(env!("CARGO_MANIFEST_DIR"), "/examples/activity.jsonl")
}

#[test]
fn renders_and_protects_existing_output() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("heatmap.svg");
    Command::cargo_bin("daytile")
        .unwrap()
        .args([
            "render",
            fixture(),
            "--output",
            output.to_str().unwrap(),
            "--year",
            "2026",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("rendered 8 populated days"));
    assert!(fs::read_to_string(&output)
        .unwrap()
        .contains("2026 calendar heatmap"));

    Command::cargo_bin("daytile")
        .unwrap()
        .args([
            "render",
            fixture(),
            "--output",
            output.to_str().unwrap(),
            "--year",
            "2026",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("File exists"));
}

#[test]
fn emits_machine_readable_summary() {
    Command::cargo_bin("daytile")
        .unwrap()
        .args(["summary", fixture(), "--year", "2026", "--aggregate", "sum"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"event_count\": 9"))
        .stdout(predicate::str::contains("\"busiest_day\": \"2026-11-10\""));
}

#[test]
fn reports_malformed_json_with_line_number() {
    let file = tempfile::NamedTempFile::new().unwrap();
    fs::write(file.path(), "{not json}\n").unwrap();
    Command::cargo_bin("daytile")
        .unwrap()
        .args(["summary", file.path().to_str().unwrap(), "--year", "2026"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("line 1"));
}
