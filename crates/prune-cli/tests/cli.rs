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
    assert_eq!(json["schema_version"], 2);
    assert_eq!(json["completeness"], "complete");
    assert_eq!(json["scope"]["root_kind"], "home");
    assert_eq!(json["scope"]["comparable"], false);
    assert_eq!(json["scope"]["min_size_bytes"], 0);
    assert_eq!(json["scope"]["categories"], serde_json::json!([]));
    assert_eq!(json["scope"]["max_risk"], serde_json::Value::Null);
    assert!(
        json["scope"]["rule_set_digest"]
            .as_str()
            .is_some_and(|digest| digest.starts_with("sha256:") && digest.len() == 71)
    );
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
fn redact_home_json_redacts_all_string_leaves_and_default_json_stays_raw() {
    let home = dirs::home_dir().expect("home directory");
    assert!(home.is_absolute(), "home directory should be absolute");
    let canonical_home = home.canonicalize().expect("canonical home directory");
    let dir = tempdir_in(&home).expect("temporary directory under home");
    let cache = dir.path().join("cache");
    fs::create_dir(&cache).expect("cache directory");
    fs::write(cache.join("a.bin"), b"12345").expect("fixture file");

    let rules_dir = dir.path().join("rules");
    fs::create_dir(&rules_dir).expect("rules directory");
    let home_native = home.to_string_lossy().into_owned();
    let home_slash = home_native.replace('\\', "/");
    let canonical_home_native = canonical_home.to_string_lossy().into_owned();
    let canonical_home_slash = canonical_home_native.replace('\\', "/");
    let cache_slash = cache.to_string_lossy().replace('\\', "/");
    let name =
        serde_json::to_string(&format!("Fixture in {home_native}")).expect("quoted rule name");
    let description = serde_json::to_string(&format!("Located under {home_slash}"))
        .expect("quoted rule description");
    let path = serde_json::to_string(&cache_slash).expect("quoted fixture path");
    let rule = format!(
        "id = \"fixture\"\nname = {name}\ndescription = {description}\nos = [\"{}\"]\ncategory = \"developer\"\nrisk = \"safe\"\nrecreate = \"automatic\"\npaths = [{path}]\n",
        std::env::consts::OS,
    );
    fs::write(rules_dir.join("fixture.toml"), rule).expect("rule file");

    let run_scan = |redact: bool| {
        let mut command = Command::cargo_bin("prune").expect("prune binary");
        command.args([
            "scan",
            "--rules-dir",
            rules_dir.to_str().expect("utf-8 rules directory"),
            "--json",
            "--workers",
            "1",
        ]);
        if redact {
            command.arg("--redact-home");
        }
        command.assert().success().get_output().to_owned()
    };

    let redacted = run_scan(true);
    assert!(
        redacted.stderr.is_empty(),
        "JSON scan must not emit progress"
    );
    let redacted: serde_json::Value =
        serde_json::from_slice(&redacted.stdout).expect("valid redacted JSON");
    let finding_path = redacted["findings"][0]["path"]
        .as_str()
        .expect("finding path");
    assert!(finding_path.starts_with("~"));
    assert_no_home_in_string_leaves(
        &redacted,
        &[
            &home_native,
            &home_slash,
            &canonical_home_native,
            &canonical_home_slash,
        ],
    );
    assert!(
        redacted["findings"][0]["rule_name"]
            .as_str()
            .expect("rule name")
            .starts_with("Fixture in ~")
    );
    assert_eq!(redacted["findings"][0]["description"], "Located under ~");

    let raw = run_scan(false);
    assert!(raw.stderr.is_empty(), "JSON scan must not emit progress");
    let raw: serde_json::Value = serde_json::from_slice(&raw.stdout).expect("valid raw JSON");
    let raw_path = raw["findings"][0]["path"].as_str().expect("finding path");
    assert!(
        raw_path.starts_with(home_native.as_str())
            || raw_path.starts_with(home_slash.as_str())
            || raw_path.starts_with(canonical_home_native.as_str())
            || raw_path.starts_with(canonical_home_slash.as_str()),
        "default JSON should preserve the absolute path"
    );
    assert!(
        raw["findings"][0]["rule_name"]
            .as_str()
            .expect("rule name")
            .contains(home_native.as_str())
    );
    assert!(
        raw["findings"][0]["description"]
            .as_str()
            .expect("description")
            .contains(home_slash.as_str())
    );
}

#[test]
fn redact_home_requires_json() {
    Command::cargo_bin("prune")
        .expect("prune binary")
        .args(["scan", "--redact-home"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("--json"));
}

#[test]
fn clap_errors_redact_home_when_the_raw_args_include_the_option() {
    let home = dirs::home_dir().expect("home directory");
    let home_native = home.to_string_lossy().into_owned();
    let invalid_workers = format!("{home_native}/not-a-worker");
    let output = Command::cargo_bin("prune")
        .expect("prune binary")
        .args([
            "scan",
            "--workers",
            invalid_workers.as_str(),
            "--json",
            "--redact-home",
        ])
        .assert()
        .code(2)
        .get_output()
        .to_owned();

    let stderr = String::from_utf8(output.stderr).expect("utf-8 clap error");
    assert!(!stderr.contains(home_native.as_str()));
    assert!(stderr.contains("~/not-a-worker"));
    assert!(output.stdout.is_empty());
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

fn assert_no_home_in_string_leaves(value: &serde_json::Value, home_forms: &[&str]) {
    match value {
        serde_json::Value::String(string) => {
            for home in home_forms {
                assert!(
                    !string.contains(home),
                    "JSON string leaf contains an unredacted home path"
                );
            }
        }
        serde_json::Value::Array(values) => {
            for value in values {
                assert_no_home_in_string_leaves(value, home_forms);
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                assert_no_home_in_string_leaves(value, home_forms);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
}

#[test]
fn scan_remember_and_history_commands_are_local_and_explicit() {
    let root = tempfile::tempdir().expect("temporary root");
    let home = root.path().join("home");
    std::fs::create_dir_all(home.join("cache")).expect("cache directory");
    std::fs::write(home.join("cache/a.bin"), b"12345").expect("fixture file");

    let rules_dir = root.path().join("rules");
    std::fs::create_dir_all(&rules_dir).expect("rules directory");
    let path = format!("{}/cache/*", home.to_string_lossy().replace('\\', "/"));
    let rule = format!(
        "id = \"fixture\"\nname = \"Fixture\"\ndescription = \"Fixture rule\"\nos = [\"{}\"]\ncategory = \"developer\"\nrisk = \"safe\"\nrecreate = \"automatic\"\npaths = [\"{}\"]\n",
        std::env::consts::OS,
        path
    );
    std::fs::write(rules_dir.join("fixture.toml"), rule).expect("rule file");

    let run_scan = |size: &[u8]| {
        std::fs::write(home.join("cache/a.bin"), size).expect("fixture file");
        let mut cmd = Command::cargo_bin("prune").expect("prune binary");
        cmd.env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("LOCALAPPDATA", home.join("AppData").join("Local"))
            .env("APPDATA", home.join("AppData").join("Roaming"))
            .args([
                "scan",
                "--rules-dir",
                rules_dir.to_str().expect("utf-8 rules dir"),
                "--json",
                "--no-tui",
                "--workers",
                "1",
                "--remember",
            ]);
        cmd.assert().success().get_output().to_owned()
    };

    let first = run_scan(b"12345");
    assert!(String::from_utf8_lossy(&first.stderr).contains("Saved scan #1"));

    let second = run_scan(b"1234567890");
    assert!(String::from_utf8_lossy(&second.stderr).contains("Saved scan #2"));

    let mut list = Command::cargo_bin("prune").expect("prune binary");
    list.env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("LOCALAPPDATA", home.join("AppData").join("Local"))
        .env("APPDATA", home.join("AppData").join("Roaming"))
        .args(["history", "list"]);
    let list_output = String::from_utf8(list.assert().success().get_output().stdout.clone())
        .expect("utf-8 history list");
    assert!(list_output.contains("1"));
    assert!(list_output.contains("2"));

    let mut diff = Command::cargo_bin("prune").expect("prune binary");
    diff.env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("LOCALAPPDATA", home.join("AppData").join("Local"))
        .env("APPDATA", home.join("AppData").join("Roaming"))
        .args(["history", "diff", "1", "2"]);
    let diff_output = String::from_utf8(diff.assert().success().get_output().stdout.clone())
        .expect("utf-8 history diff");
    assert!(
        diff_output.contains("no longer observed")
            || diff_output.contains("changed")
            || diff_output.contains("new")
    );

    Command::cargo_bin("prune")
        .expect("prune binary")
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("LOCALAPPDATA", home.join("AppData").join("Local"))
        .env("APPDATA", home.join("AppData").join("Roaming"))
        .args(["history", "delete", "1"])
        .assert()
        .success();

    Command::cargo_bin("prune")
        .expect("prune binary")
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("LOCALAPPDATA", home.join("AppData").join("Local"))
        .env("APPDATA", home.join("AppData").join("Roaming"))
        .args(["history", "clear"])
        .assert()
        .failure();

    Command::cargo_bin("prune")
        .expect("prune binary")
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("LOCALAPPDATA", home.join("AppData").join("Local"))
        .env("APPDATA", home.join("AppData").join("Roaming"))
        .args(["history", "clear", "--yes"])
        .assert()
        .success();
}
