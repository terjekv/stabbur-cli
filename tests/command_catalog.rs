use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn top_level_help_contains_complete_v01_catalog() {
    let mut command = Command::cargo_bin("stabbur").unwrap();
    command.arg("--help");
    command.assert().success().stdout(
        predicate::str::contains("auth")
            .and(predicate::str::contains("bootstrap"))
            .and(predicate::str::contains("catalog"))
            .and(predicate::str::contains("target"))
            .and(predicate::str::contains("software"))
            .and(predicate::str::contains("release"))
            .and(predicate::str::contains("variant"))
            .and(predicate::str::contains("channel"))
            .and(predicate::str::contains("recipe"))
            .and(predicate::str::contains("run"))
            .and(predicate::str::contains("artifact"))
            .and(predicate::str::contains("storage"))
            .and(predicate::str::contains("worker"))
            .and(predicate::str::contains("job"))
            .and(predicate::str::contains("audit"))
            .and(predicate::str::contains("resolve")),
    );
}

#[test]
fn catalog_exposes_non_mutating_plan_and_idempotent_sync() {
    let mut command = Command::cargo_bin("stabbur").unwrap();
    command.args(["catalog", "--help"]);
    command
        .assert()
        .success()
        .stdout(predicate::str::contains("plan"))
        .stdout(predicate::str::contains("sync"))
        .stdout(predicate::str::contains("snapshots"))
        .stdout(predicate::str::contains("show-snapshot"))
        .stdout(predicate::str::contains("resolve"))
        .stdout(predicate::str::contains("scan"));

    for operation in ["plan", "sync"] {
        let mut command = Command::cargo_bin("stabbur").unwrap();
        command.args(["catalog", operation, "--help"]);
        command
            .assert()
            .success()
            .stdout(predicate::str::contains("--file"));
    }
}

#[test]
fn target_exposes_desired_policy_and_run_operations() {
    let mut command = Command::cargo_bin("stabbur").unwrap();
    command.args(["target", "--help"]);
    command
        .assert()
        .success()
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("show"))
        .stdout(predicate::str::contains("create"))
        .stdout(predicate::str::contains("update"))
        .stdout(predicate::str::contains("trigger"))
        .stdout(predicate::str::contains("runs"));
}

#[test]
fn bootstrap_accepts_protected_sources_but_not_secret_values() {
    let mut command = Command::cargo_bin("stabbur").unwrap();
    command.args(["bootstrap", "--help"]);
    command
        .assert()
        .success()
        .stdout(predicate::str::contains("--bootstrap-secret-file"))
        .stdout(predicate::str::contains("--password-file"))
        .stdout(predicate::str::contains("--secret <").not())
        .stdout(predicate::str::contains("--password <").not());
}

#[test]
fn auth_exposes_bootstrap_and_short_password_reset() {
    let mut command = Command::cargo_bin("stabbur").unwrap();
    command.args(["auth", "--help"]);
    command
        .assert()
        .success()
        .stdout(predicate::str::contains("bootstrap"))
        .stdout(predicate::str::contains("reset-password"));
}

#[test]
fn login_accepts_password_sources_but_not_password_values() {
    let mut command = Command::cargo_bin("stabbur").unwrap();
    command.args(["auth", "login", "--help"]);
    command
        .assert()
        .success()
        .stdout(predicate::str::contains("--password-file"))
        .stdout(predicate::str::contains("--password <").not());
}

#[test]
fn manifest_has_exact_client_and_no_direct_http_dependency() {
    let manifest = std::fs::read_to_string("Cargo.toml").unwrap();
    assert!(manifest.contains("version = \"=0.0.1\""));
    assert!(manifest.contains("features = [\"blocking\"]"));
    assert!(!manifest.lines().any(|line| line.starts_with("reqwest")));
}
