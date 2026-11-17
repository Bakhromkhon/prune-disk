use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir_in;

#[test]
fn validates_builtin_rules() {
    let mut cmd = Command::cargo_bin("prune").expect("prune binary");
    cmd.args(["rules", "validate"]);
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("rules are valid"));
}

#[test]
fn doctor_is_explicit_about_privacy() {
    let mut cmd = Command::cargo_bin("prune").expect("prune binary");
    cmd.arg("doctor");
    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Network: disabled by design"))
        .stdout(predicate::str::contains("Telemetry: none"));
}

#[test]
fn scans_a_fixture_and_returns_json() {
    let home = dirs::home_dir().expect("home directory");
    let dir = tempdir_in(&home).expect("temporary directory under home");
    let cache = dir.path().join("cache");
    fs::create_dir(&cache).expect("cache directory");
    fs::write(cache.join("a.bin"), b"12345").expect("fixture file");

    let rules_dir = dir.path().join("rules");
    fs::create_dir(&rules_dir).expect("rules directory");
    let path = cache.to_string_lossy().replace('\\', "/");
    let rule = format!(
        "id = \"fixture\"\nname = \"Fixture\"\ndescription = \"Fixture rule\"\nos = [\"{}\"]\ncategory = \"developer\"\nrisk = \"safe\"\nrecreate = \"automatic\"\npaths = [\"{}\"]\n",
        std::env::consts::OS,
        path
    );
    fs::write(rules_dir.join("fixture.toml"), rule).expect("rule file");

    let output = Command::cargo_bin("prune")
        .expect("prune binary")
        .args([
            "scan",
            "--rules-dir",
            rules_dir.to_str().expect("utf-8 rules dir"),
            "--json",
            "--no-tui",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let json: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(json["findings"][0]["size_bytes"], 5);
    assert_eq!(json["findings"][0]["file_count"], 1);
}
