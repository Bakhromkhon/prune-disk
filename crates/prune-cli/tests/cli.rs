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
        .stdout(predicate::str::contains(
            "no intentional network functionality",
        ))
        .stdout(predicate::str::contains("Telemetry: none"));
}

#[test]
fn argument_errors_escape_control_and_bidi_characters() {
    let output = Command::cargo_bin("prune")
        .expect("prune binary")
        .args(["scan", "--min-size", "bad\nFORGED\u{202e}"])
        .assert()
        .failure()
        .get_output()
        .stderr
        .clone();
    let text = String::from_utf8(output).expect("utf-8 error");
    assert!(!text.contains('\u{202e}'));
    assert!(!text.contains("\nFORGED"));
    assert!(text.contains("\\nFORGED"));
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
            "--workers",
            "1",
        ])
        .assert()
        .success()
        .get_output()
        .to_owned();
    assert!(output.stderr.is_empty(), "JSON scan must not emit progress");
    assert!(!output.stdout.contains(&0x1b));
    let output = output.stdout.clone();

    let json: serde_json::Value = serde_json::from_slice(&output).expect("valid JSON");
    assert_eq!(json["findings"][0]["size_bytes"], 5);
    assert_eq!(json["findings"][0]["file_count"], 1);
    assert_eq!(json["cancelled"], false);
    assert_eq!(json["suppressed_errors"], 0);

    let human = Command::cargo_bin("prune")
        .expect("prune binary")
        .args([
            "scan",
            "--rules-dir",
            rules_dir.to_str().expect("utf-8 rules dir"),
            "--workers",
            "16",
        ])
        .assert()
        .success()
        .get_output()
        .to_owned();
    assert!(human.stderr.is_empty());
    assert!(!human.stdout.contains(&0x1b));
    assert!(String::from_utf8(human.stdout).unwrap().contains("Fixture"));
}

#[test]
fn rejects_workers_outside_bounded_range() {
    for workers in ["0", "17", "256", "-1", "two"] {
        Command::cargo_bin("prune")
            .expect("prune binary")
            .args(["scan", "--workers", workers, "--json"])
            .assert()
            .code(2)
            .stdout(predicate::str::is_empty());
    }
}

#[test]
fn scan_help_documents_cancellation_exit_and_noninteractive_limits() {
    Command::cargo_bin("prune")
        .expect("prune binary")
        .args(["scan", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("exit code 130"))
        .stdout(predicate::str::contains("default signal behavior"))
        .stdout(predicate::str::contains("--workers"));
}
