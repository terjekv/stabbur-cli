use std::{
    fs::OpenOptions,
    io::{Read, Write},
    net::TcpListener,
    path::Path,
    thread,
};

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn resolver_aliases_use_the_contract_wire_names() {
    let (server, request) = mock_once_response("404 Not Found", "");
    let directory = tempfile::tempdir().unwrap();
    Command::cargo_bin("stabbur")
        .unwrap()
        .args([
            "--server",
            &server,
            "--profile",
            directory.path().join("absent").to_str().unwrap(),
            "resolve",
            "example",
            "--platform",
            "macos",
            "--architecture",
            "arm64",
        ])
        .env("STABBUR_TOKEN", "fixture-only-token")
        .assert()
        .failure();
    let request = request.join().unwrap();
    assert!(request.contains("platform=mac_os"));
    assert!(request.contains("architecture=aarch64"));
}

#[test]
fn resolver_rejects_unknown_and_nonconcrete_targets_before_authentication() {
    Command::cargo_bin("stabbur")
        .unwrap()
        .args([
            "resolve",
            "example",
            "--platform",
            "mac_os",
            "--architecture",
            "universal",
        ])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalid value 'universal'"));
}

fn mock_once(body: &'static str) -> (String, thread::JoinHandle<String>) {
    mock_once_response("200 OK", body)
}

fn mock_once_response(
    status: &'static str,
    body: &'static str,
) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let task = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 4096];
        let header_end = loop {
            match stream.read(&mut buffer) {
                Ok(0) => panic!("mock request ended before its headers"),
                Ok(count) => {
                    request.extend_from_slice(&buffer[..count]);
                    if let Some(position) =
                        request.windows(4).position(|value| value == b"\r\n\r\n")
                    {
                        break position + 4;
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    panic!("mock request headers timed out")
                }
                Err(error) => panic!("mock read failed: {error}"),
            }
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().unwrap())
                })
            })
            .unwrap_or(0);
        while request.len() < header_end + content_length {
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0, "mock request body ended early");
            request.extend_from_slice(&buffer[..count]);
        }
        let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\nx-request-id: cli-contract-test\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(response.as_bytes()).unwrap();
        String::from_utf8(request).unwrap()
    });
    (format!("http://{address}"), task)
}

fn mock_sequence(
    responses: Vec<(&'static str, &'static str)>,
) -> (String, thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let task = thread::spawn(move || {
        responses
            .into_iter()
            .map(|(status, body)| {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 4096];
                let header_end = loop {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0, "mock request ended before its headers");
                    request.extend_from_slice(&buffer[..count]);
                    if let Some(position) =
                        request.windows(4).position(|value| value == b"\r\n\r\n")
                    {
                        break position + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.split_once(':').and_then(|(name, value)| {
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                    })
                    .unwrap_or(0);
                while request.len() < header_end + content_length {
                    let count = stream.read(&mut buffer).unwrap();
                    assert!(count > 0, "mock request body ended early");
                    request.extend_from_slice(&buffer[..count]);
                }
                let content_type = if body.starts_with("event:") { "text/event-stream" } else { "application/json" };
                let response = format!(
                    "HTTP/1.1 {status}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\nx-request-id: cli-catalog-test\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).unwrap();
                String::from_utf8(request).unwrap()
            })
            .collect()
    });
    (format!("http://{address}"), task)
}

fn write_secret(path: &Path, value: &str) {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).unwrap();
    writeln!(file, "{value}").unwrap();
}

#[test]
fn bootstrap_is_unauthenticated_non_interactive_and_redacted() {
    let response = r#"{"id":"01900000-0000-7000-8000-000000000001","name":"admin","kind":"human","roles":["admin"]}"#;
    let (server, request) = mock_once(response);
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("missing-profile.json");
    let secret_file = directory.path().join("bootstrap.secret");
    let password_file = directory.path().join("admin.password");
    let secret = "stb_bootstrap-contract-secret";
    let password = "automation password phrase";
    write_secret(&secret_file, secret);
    write_secret(&password_file, password);

    let mut command = Command::cargo_bin("stabbur").unwrap();
    command
        .args([
            "--server",
            &server,
            "--profile",
            profile.to_str().unwrap(),
            "--json",
            "bootstrap",
            "--username",
            "admin",
            "--bootstrap-secret-file",
            secret_file.to_str().unwrap(),
            "--password-file",
            password_file.to_str().unwrap(),
        ])
        .env("STABBUR_TOKEN", "ignored-bootstrap-bearer");
    let assertion = command.assert().success();
    let output: serde_json::Value = serde_json::from_slice(&assertion.get_output().stdout).unwrap();
    assert_eq!(output["name"], "admin");
    assert_eq!(output["roles"], serde_json::json!(["admin"]));
    assert!(!String::from_utf8_lossy(&assertion.get_output().stdout).contains(secret));
    assert!(!String::from_utf8_lossy(&assertion.get_output().stderr).contains(password));

    let request = request.join().unwrap();
    assert!(request.starts_with("POST /api/v1/auth/bootstrap HTTP/1.1\r\n"));
    assert!(!request.to_ascii_lowercase().contains("authorization:"));
    let body: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["secret"], secret);
    assert_eq!(body["username"], "admin");
    assert_eq!(body["password"], password);
}

#[test]
fn authenticated_reset_password_alias_uses_the_identity_gateway() {
    let (server, request) = mock_once_response("204 No Content", "");
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("missing-profile.json");
    let password_file = directory.path().join("admin.password");
    let password = "replacement password phrase";
    write_secret(&password_file, password);

    let mut command = Command::cargo_bin("stabbur").unwrap();
    command
        .args([
            "--server",
            &server,
            "--profile",
            profile.to_str().unwrap(),
            "auth",
            "reset-password",
            "admin",
            "--password-file",
            password_file.to_str().unwrap(),
        ])
        .env("STABBUR_TOKEN", "fixture-cli-token");
    command.assert().success();

    let request = request.join().unwrap();
    assert!(request.starts_with("POST /api/v1/auth/principals/admin/password HTTP/1.1\r\n"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer fixture-cli-token\r\n")
    );
    let body: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["new_password"], password);
}

#[test]
fn bootstrap_without_protected_inputs_fails_before_network_access() {
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("missing-profile.json");
    let mut command = Command::cargo_bin("stabbur").unwrap();
    command.args([
        "--server",
        "http://127.0.0.1:9",
        "--profile",
        profile.to_str().unwrap(),
        "bootstrap",
        "--username",
        "admin",
    ]);
    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("protected input file is required"));
}

#[test]
fn software_json_output_is_stable_and_uses_the_client_gateway() {
    let response = r#"{"items":[{"id":"01900000-0000-7000-8000-000000000001","slug":"firefox","name":"Firefox ESR","created_at":"2026-01-01T00:00:00Z","revision":3,"installation":null}],"next_cursor":null}"#;
    let (server, request) = mock_once(response);
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("missing-profile.json");
    let mut command = Command::cargo_bin("stabbur").unwrap();
    command
        .args([
            "--server",
            &server,
            "--profile",
            profile.to_str().unwrap(),
            "--json",
            "software",
            "list",
        ])
        .env("STABBUR_TOKEN", "fixture-cli-token");
    let assertion = command.assert().success();
    let output: serde_json::Value = serde_json::from_slice(&assertion.get_output().stdout).unwrap();
    assert_eq!(output["items"].as_array().unwrap().len(), 1);
    assert_eq!(output["items"][0]["slug"], "firefox");
    assert_eq!(output["items"][0]["revision"], 3);

    let request = request.join().unwrap();
    assert!(request.starts_with("GET /api/v1/software?limit=50 HTTP/1.1\r\n"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("authorization: bearer fixture-cli-token\r\n")
    );
}

#[test]
fn recipe_revision_uses_the_builder_neutral_client_contract() {
    let response = r#"{"id":"01900000-0000-7000-8000-000000000010","recipe_id":"01900000-0000-7000-8000-000000000011","sequence":1,"builder":"fake","definition":{},"required_capabilities":["builder.fake","runtime.portable"],"created_at":"2026-01-01T00:00:00Z"}"#;
    let (server, request) = mock_once(response);
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("missing-profile.json");
    let revision_file = directory.path().join("fake-revision.json");
    std::fs::write(
        &revision_file,
        r#"{"builder":"fake","definition":{},"required_capabilities":[]}"#,
    )
    .unwrap();

    let mut command = Command::cargo_bin("stabbur").unwrap();
    command
        .args([
            "--server",
            &server,
            "--profile",
            profile.to_str().unwrap(),
            "--json",
            "recipe",
            "create-revision",
            "portable",
            "--file",
            revision_file.to_str().unwrap(),
            "--idempotency-key",
            "portable-fake-v1",
        ])
        .env("STABBUR_TOKEN", "fixture-cli-token");
    let assertion = command.assert().success();
    let output: serde_json::Value = serde_json::from_slice(&assertion.get_output().stdout).unwrap();
    assert_eq!(output["builder"], "fake");

    let request = request.join().unwrap();
    assert!(request.starts_with("POST /api/v1/recipes/portable/revisions HTTP/1.1\r\n"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("idempotency-key: portable-fake-v1\r\n")
    );
    let body: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(
        body,
        serde_json::json!({
            "builder": "fake",
            "definition": {},
            "required_capabilities": []
        })
    );
}

#[test]
fn catalog_sync_creates_missing_resources_in_dependency_order() {
    let software_page = r#"{"items":[],"next_cursor":null}"#;
    let recipe_page = r#"{"items":[],"next_cursor":null}"#;
    let software = r#"{"id":"01900000-0000-7000-8000-000000000020","slug":"portable","name":"Portable","created_at":"2026-01-01T00:00:00Z","revision":1,"installation":null}"#;
    let recipe = r#"{"id":"01900000-0000-7000-8000-000000000021","name":"portable","created_at":"2026-01-01T00:00:00Z","revision":1}"#;
    let revision = r#"{"id":"01900000-0000-7000-8000-000000000022","recipe_id":"01900000-0000-7000-8000-000000000021","sequence":1,"builder":"fake","definition":{},"required_capabilities":["builder.fake","runtime.portable"],"created_at":"2026-01-01T00:00:00Z"}"#;
    let (server, requests) = mock_sequence(vec![
        ("200 OK", software_page),
        ("200 OK", recipe_page),
        ("201 Created", software),
        ("201 Created", recipe),
        ("201 Created", revision),
    ]);
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("missing-profile.json");
    let manifest = directory.path().join("catalog.json");
    std::fs::write(
        &manifest,
        r#"{
          "schema_version": 1,
          "software": [{"slug":"portable","name":"Portable"}],
          "recipes": [{
            "name":"portable",
            "revision":{"builder":"fake","definition":{},"required_capabilities":[]}
          }]
        }"#,
    )
    .unwrap();

    let mut command = Command::cargo_bin("stabbur").unwrap();
    command
        .args([
            "--server",
            &server,
            "--profile",
            profile.to_str().unwrap(),
            "--json",
            "catalog",
            "sync",
            "--file",
            manifest.to_str().unwrap(),
        ])
        .env("STABBUR_TOKEN", "fixture-cli-token");
    let assertion = command.assert().success();
    let output: serde_json::Value = serde_json::from_slice(&assertion.get_output().stdout).unwrap();
    assert_eq!(output["schema_version"], 1);
    assert_eq!(output["applied"].as_array().unwrap().len(), 3);
    assert_eq!(output["applied"][0]["action"], "create_software");
    assert_eq!(output["applied"][1]["action"], "create_recipe");
    assert_eq!(output["applied"][2]["action"], "create_recipe_revision");

    let requests = requests.join().unwrap();
    assert!(requests[0].starts_with("GET /api/v1/software?limit=200 HTTP/1.1\r\n"));
    assert!(requests[1].starts_with("GET /api/v1/recipes?limit=200 HTTP/1.1\r\n"));
    assert!(requests[2].starts_with("POST /api/v1/software HTTP/1.1\r\n"));
    assert!(requests[3].starts_with("POST /api/v1/recipes HTTP/1.1\r\n"));
    assert!(requests[4].starts_with("POST /api/v1/recipes/portable/revisions HTTP/1.1\r\n"));
    let revision_body: serde_json::Value =
        serde_json::from_str(requests[4].split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(
        revision_body["required_capabilities"],
        serde_json::json!(["builder.fake", "runtime.portable"])
    );
}

#[test]
fn invalid_catalog_fails_before_network_access() {
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("missing-profile.json");
    let manifest = directory.path().join("catalog.json");
    std::fs::write(
        &manifest,
        r#"{"schema_version":99,"software":[],"recipes":[]}"#,
    )
    .unwrap();

    let mut command = Command::cargo_bin("stabbur").unwrap();
    command
        .args([
            "--server",
            "http://127.0.0.1:9",
            "--profile",
            profile.to_str().unwrap(),
            "catalog",
            "plan",
            "--file",
            manifest.to_str().unwrap(),
        ])
        .env("STABBUR_TOKEN", "catalog-validation-secret");
    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("catalog schema version"))
        .stderr(predicate::str::contains("catalog-validation-secret").not())
        .stderr(predicate::str::contains("transport failed").not());
}

#[test]
fn catalog_scan_request_is_pinned_idempotent_and_uses_the_client_gateway() {
    let response = r#"{
      "id":"01900000-0000-7000-8000-000000000030",
      "job_id":"01900000-0000-7000-8000-000000000031",
      "producer":"autopkg",
      "source":{"locator":"https://example.test/recipes.git","revision":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"},
      "state":"queued",
      "snapshot_id":null,
      "failure":null,
      "requested_at":"2026-01-01T00:00:00Z",
      "completed_at":null
    }"#;
    let (server, request) = mock_once_response("201 Created", response);
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("missing-profile.json");

    let mut command = Command::cargo_bin("stabbur").unwrap();
    command
        .args([
            "--server",
            &server,
            "--profile",
            profile.to_str().unwrap(),
            "--json",
            "catalog",
            "scan",
            "request",
            "--source-url",
            "https://example.test/recipes.git",
            "--source-revision",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "--idempotency-key",
            "catalog-scan-a",
        ])
        .env("STABBUR_TOKEN", "fixture-cli-token");
    let assertion = command.assert().success();
    let output: serde_json::Value = serde_json::from_slice(&assertion.get_output().stdout).unwrap();
    assert_eq!(output["producer"], "autopkg");
    assert_eq!(output["state"], "queued");

    let request = request.join().unwrap();
    assert!(request.starts_with("POST /api/v1/recipe-catalog-scans HTTP/1.1\r\n"));
    assert!(
        request
            .to_ascii_lowercase()
            .contains("idempotency-key: catalog-scan-a\r\n")
    );
    let body: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["producer"], "autopkg");
    assert_eq!(
        body["source"]["revision"],
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
    );
}

#[test]
fn build_target_creation_uses_typed_manual_policy() {
    let response = r#"{
      "id":"01900000-0000-7000-8000-000000000040",
      "name":"firefox-nightly",
      "software_id":"01900000-0000-7000-8000-000000000041",
      "recipe_revision_id":"01900000-0000-7000-8000-000000000042",
      "parameters":{},
      "schedule":{"kind":"manual"},
      "enabled":true,
      "next_run_at":null,
      "created_at":"2026-01-01T00:00:00Z",
      "updated_at":"2026-01-01T00:00:00Z",
      "revision":1
    }"#;
    let (server, request) = mock_once_response("201 Created", response);
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("missing-profile.json");

    let mut command = Command::cargo_bin("stabbur").unwrap();
    command
        .args([
            "--server",
            &server,
            "--profile",
            profile.to_str().unwrap(),
            "--json",
            "target",
            "create",
            "--name",
            "firefox-nightly",
            "--software",
            "firefox",
            "--recipe-revision",
            "01900000-0000-7000-8000-000000000042",
        ])
        .env("STABBUR_TOKEN", "fixture-cli-token");
    let assertion = command.assert().success();
    let output: serde_json::Value = serde_json::from_slice(&assertion.get_output().stdout).unwrap();
    assert_eq!(output["name"], "firefox-nightly");
    assert_eq!(output["schedule"]["kind"], "manual");

    let request = request.join().unwrap();
    assert!(request.starts_with("POST /api/v1/build-targets HTTP/1.1\r\n"));
    let body: serde_json::Value =
        serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["schedule"]["kind"], "manual");
    assert_eq!(
        body["recipe_revision"],
        "01900000-0000-7000-8000-000000000042"
    );
}

#[test]
fn destructive_command_decline_happens_before_network_access() {
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("missing-profile.json");
    let secret = "decline-test-secret";
    let mut command = Command::cargo_bin("stabbur").unwrap();
    command
        .args([
            "--server",
            "http://127.0.0.1:9",
            "--profile",
            profile.to_str().unwrap(),
            "release",
            "reject",
            "01900000-0000-7000-8000-000000000002",
            "--revision",
            "1",
            "--reason",
            "not approved",
        ])
        .env("STABBUR_TOKEN", secret)
        .write_stdin("no\n");
    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("operation was not confirmed"))
        .stderr(predicate::str::contains(secret).not())
        .stdout(predicate::str::contains(secret).not());
}

#[test]
fn transport_failures_never_echo_environment_credentials() {
    let directory = tempfile::tempdir().unwrap();
    let profile = directory.path().join("missing-profile.json");
    let secret = "transport-test-secret";
    let mut command = Command::cargo_bin("stabbur").unwrap();
    command
        .args([
            "--server",
            "http://127.0.0.1:9",
            "--profile",
            profile.to_str().unwrap(),
            "software",
            "show",
            "firefox",
        ])
        .env("STABBUR_TOKEN", secret);
    command
        .assert()
        .failure()
        .stderr(predicate::str::contains("Stabbur transport failed"))
        .stderr(predicate::str::contains(secret).not())
        .stdout(predicate::str::contains(secret).not());
}

#[test]
fn paginated_json_preserves_cursor_and_all_rejects_a_cycle() {
    let response = r#"{"items":[],"next_cursor":"repeat"}"#;
    let (server, requests) = mock_sequence(vec![("200 OK", response), ("200 OK", response)]);
    let directory = tempfile::tempdir().unwrap();
    Command::cargo_bin("stabbur")
        .unwrap()
        .args([
            "--server",
            &server,
            "--profile",
            directory.path().join("missing").to_str().unwrap(),
            "--json",
            "software",
            "list",
            "--all",
        ])
        .env("STABBUR_TOKEN", "pagination-fixture")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("repeated pagination cursor"));
    assert!(requests.join().unwrap()[1].contains("cursor=repeat"));
    let (server, request) = mock_once(response);
    let assertion = Command::cargo_bin("stabbur")
        .unwrap()
        .args([
            "--server",
            &server,
            "--profile",
            directory.path().join("missing").to_str().unwrap(),
            "--json",
            "software",
            "list",
        ])
        .env("STABBUR_TOKEN", "pagination-fixture")
        .assert()
        .success();
    let result: serde_json::Value = serde_json::from_slice(&assertion.get_output().stdout).unwrap();
    assert_eq!(result["next_cursor"], "repeat");
    request.join().unwrap();
}

#[test]
fn corrupt_download_does_not_replace_an_existing_destination() {
    let metadata = r#"{"digest":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad","size":3,"media_type":"application/octet-stream","created_at":"2026-01-01T00:00:00Z","locations":[]}"#;
    let (server, requests) = mock_sequence(vec![("200 OK", metadata), ("200 OK", "bad")]);
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("installer.pkg");
    std::fs::write(&output, b"previous bytes").unwrap();
    Command::cargo_bin("stabbur")
        .unwrap()
        .args([
            "--server",
            &server,
            "--profile",
            directory.path().join("missing").to_str().unwrap(),
            "artifact",
            "download",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            "--output",
            output.to_str().unwrap(),
            "--overwrite",
        ])
        .env("STABBUR_TOKEN", "download-fixture")
        .assert()
        .failure()
        .stderr(predicate::str::contains("SHA-256"));
    assert_eq!(std::fs::read(&output).unwrap(), b"previous bytes");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    requests.join().unwrap();
}

#[test]
fn waiting_for_a_failed_run_has_a_distinct_exit_status() {
    let run = r#"{"id":"01900000-0000-7000-8000-000000000001","recipe_revision_id":"01900000-0000-7000-8000-000000000002","software_id":"01900000-0000-7000-8000-000000000003","state":"failed","parameters":{},"result":null,"created_at":"2026-01-01T00:00:00Z","completed_at":"2026-01-01T00:00:01Z"}"#;
    let (server, request) = mock_once(run);
    let directory = tempfile::tempdir().unwrap();
    Command::cargo_bin("stabbur")
        .unwrap()
        .args([
            "--server",
            &server,
            "--profile",
            directory.path().join("missing").to_str().unwrap(),
            "--json",
            "run",
            "wait",
            "01900000-0000-7000-8000-000000000001",
            "--timeout-seconds",
            "1",
        ])
        .env("STABBUR_TOKEN", "wait-fixture")
        .assert()
        .code(2)
        .stdout(predicate::str::contains("failed"))
        .stderr(predicate::str::contains("\"exit_code\":2"));
    request.join().unwrap();
}

#[test]
fn trigger_watch_follows_the_returned_run_and_preserves_failure_exit() {
    let run = r#"{"id":"01900000-0000-7000-8000-000000000001","recipe_revision_id":"01900000-0000-7000-8000-000000000002","software_id":"01900000-0000-7000-8000-000000000003","state":"queued","parameters":{},"result":null,"created_at":"2026-01-01T00:00:00Z","completed_at":null}"#;
    let complete = "event: complete\ndata: {\"run_id\":\"01900000-0000-7000-8000-000000000001\",\"state\":\"failed\"}\n\n";
    let (server, requests) = mock_sequence(vec![("201 Created", run), ("200 OK", complete)]);
    let directory = tempfile::tempdir().unwrap();
    Command::cargo_bin("stabbur")
        .unwrap()
        .args([
            "--server",
            &server,
            "--profile",
            directory.path().join("missing").to_str().unwrap(),
            "target",
            "trigger",
            "nightly",
            "--idempotency-key",
            "watch-once",
            "--watch",
            "--timeout-seconds",
            "5",
        ])
        .env("STABBUR_TOKEN", "fixture-token")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("Following run"))
        .stdout(predicate::str::contains("failed"));
    let requests = requests.join().unwrap();
    assert!(requests[0].starts_with("POST /api/v1/build-targets/nightly/runs "));
    assert!(
        requests[1].starts_with("GET /api/v1/runs/01900000-0000-7000-8000-000000000001/events ")
    );
}

#[test]
fn fetched_promotion_revision_is_fenced_and_not_retried_after_conflict() {
    let channel = r#"{"software_id":"01900000-0000-7000-8000-000000000003","name":"stable","release_id":"01900000-0000-7000-8000-000000000002","pinned_variant_id":null,"revision":7}"#;
    let conflict = r#"{"code":"stale_revision","status":412,"detail":"The resource changed since it was read.","request_id":"revision-test","validation_errors":[]}"#;
    let (server, requests) = mock_sequence(vec![
        ("200 OK", channel),
        ("412 Precondition Failed", conflict),
    ]);
    let directory = tempfile::tempdir().unwrap();
    Command::cargo_bin("stabbur")
        .unwrap()
        .args([
            "--server",
            &server,
            "--profile",
            directory.path().join("missing").to_str().unwrap(),
            "--yes",
            "release",
            "promote",
            "firefox",
            "01900000-0000-7000-8000-000000000004",
            "--channel",
            "stable",
            "--current-revision",
        ])
        .env("STABBUR_TOKEN", "fixture-token")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("review the change"))
        .stderr(predicate::str::contains("Problem {").not());
    let requests = requests.join().unwrap();
    assert!(requests[0].starts_with("GET /api/v1/software/firefox/channels/stable "));
    assert!(requests[1].to_lowercase().contains("if-match: \"rev-7\""));
}

#[test]
fn human_errors_show_field_diagnostics_and_keep_json_error_envelope() {
    let problem = r#"{"code":"validation_failed","status":400,"detail":"The slug is invalid.","request_id":"input-test","validation_errors":[{"field":"slug","code":"invalid","message":"Use lowercase letters."}]}"#;
    for json in [false, true] {
        let (server, request) = mock_once_response("400 Bad Request", problem);
        let directory = tempfile::tempdir().unwrap();
        let mut command = Command::cargo_bin("stabbur").unwrap();
        command
            .args([
                "--server",
                &server,
                "--profile",
                directory.path().join("missing").to_str().unwrap(),
                "software",
                "show",
                "invalid",
            ])
            .env("STABBUR_TOKEN", "fixture-token");
        if json {
            command.arg("--json");
        }
        let assertion = command.assert().code(1);
        let stderr = String::from_utf8_lossy(&assertion.get_output().stderr);
        if json {
            let value: serde_json::Value = serde_json::from_str(&stderr).unwrap();
            assert_eq!(value["error"]["exit_code"], 1);
            assert!(value["error"]["message"].is_string());
        } else {
            assert!(stderr.contains("slug: Use lowercase letters."));
            assert!(stderr.contains("Reference: input-test"));
            assert!(!stderr.contains("Problem {"));
        }
        request.join().unwrap();
    }
}
