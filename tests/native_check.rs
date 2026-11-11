#![cfg(target_os = "macos")]
use arc_mcp::{
    evidence::{CheckExecutor, CheckRequest},
    native_check::NativeCheckExecutor,
};
use std::{fs, path::Path, time::Duration};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

fn fixture() -> (TempDir, NativeCheckExecutor) {
    let temp = tempfile::tempdir().unwrap();
    for path in ["project", "project/protected", "private"] {
        fs::create_dir_all(temp.path().join(path)).unwrap();
    }
    fs::write(temp.path().join("project/protected/file"), "original").unwrap();
    fs::write(temp.path().join("private/secret"), "private fixture").unwrap();
    let runner = NativeCheckExecutor::new(
        vec![temp.path().join("project/protected")],
        vec![temp.path().join("private")],
    )
    .unwrap();
    (temp, runner)
}

fn request(root: &Path, command: &str, cancellation: CancellationToken) -> CheckRequest {
    CheckRequest {
        project_root: root.join("project"),
        command: command.into(),
        timeout_secs: 3,
        max_output_bytes: 4096,
        cancellation,
    }
}

#[tokio::test]
async fn real_commands_write_project_capture_exit_and_protect_aliases() {
    let (temp, runner) = fixture();
    let root = temp.path();
    let allowed = runner
        .execute(request(
            root,
            "printf useful > result; printf stdout; printf stderr >&2; exit 7",
            CancellationToken::new(),
        ))
        .await
        .unwrap();
    assert_eq!(allowed.exit_code, Some(7));
    assert_eq!(allowed.stdout, "stdout");
    assert_eq!(allowed.stderr, "stderr");
    assert_eq!(
        fs::read_to_string(root.join("project/result")).unwrap(),
        "useful"
    );
    std::os::unix::fs::symlink(
        root.join("project/protected/file"),
        root.join("project/alias"),
    )
    .unwrap();
    for command in [
        "printf bad > protected/file",
        "printf bad > alias",
        "rm -rf protected",
        "mv protected moved",
        "ln protected/file newalias && printf bad > newalias",
        "cat ../private/secret",
        "printf bad > ../outside",
        "curl --max-time 1 http://127.0.0.1:1",
    ] {
        let result = runner
            .execute(request(root, command, CancellationToken::new()))
            .await
            .unwrap();
        assert_ne!(
            result.exit_code,
            Some(0),
            "unexpected success: {command} {result:?}"
        );
        assert_eq!(
            fs::read_to_string(root.join("project/protected/file")).unwrap(),
            "original"
        );
    }
}

#[tokio::test]
async fn cancellation_timeout_output_bounds_and_background_group_cleanup() {
    let (temp, runner) = fixture();
    let token = CancellationToken::new();
    let cancel = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        cancel.cancel();
    });
    let result = runner
        .execute(request(
            temp.path(),
            "sleep 2; printf escaped > late",
            token,
        ))
        .await
        .unwrap();
    assert!(result.cancelled);
    let result = runner
        .execute(request(
            temp.path(),
            "(sleep 1; printf escaped > background) & exit 0",
            CancellationToken::new(),
        ))
        .await
        .unwrap();
    assert_eq!(result.exit_code, Some(0));
    let mut timed = request(temp.path(), "sleep 4", CancellationToken::new());
    timed.timeout_secs = 1;
    assert!(runner.execute(timed).await.unwrap().timed_out);
    assert!(!temp.path().join("project/late").exists());
    assert!(!temp.path().join("project/background").exists());
    let result = runner
        .execute(request(
            temp.path(),
            "i=0; while [ $i -lt 10000 ]; do printf x; i=$((i+1)); done",
            CancellationToken::new(),
        ))
        .await
        .unwrap();
    assert!(result.truncated, "{result:?}");
    assert!(result.stdout.len() <= 2048);
}

#[test]
fn preexisting_hardlinks_refuse_guard_startup() {
    let (temp, _) = fixture();
    fs::hard_link(
        temp.path().join("project/protected/file"),
        temp.path().join("project/hardlink"),
    )
    .unwrap();
    assert!(NativeCheckExecutor::new(vec![temp.path().join("project/protected")], vec![]).is_err());
}

#[tokio::test]
async fn existing_node_and_cargo_can_run_real_offline_tests() {
    let (temp, runner) = fixture();
    fs::write(temp.path().join("project/sample.test.cjs"), "const test = require('node:test'); const assert = require('node:assert/strict'); test('answer', () => assert.equal(6 * 7, 42));").unwrap();
    fs::create_dir_all(temp.path().join("project/rust/src")).unwrap();
    fs::write(
        temp.path().join("project/rust/Cargo.toml"),
        "[package]\nname='arc-disposable-check'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    fs::write(
        temp.path().join("project/rust/src/lib.rs"),
        "#[test] fn answer() { assert_eq!(6 * 7, 42); }",
    )
    .unwrap();
    for command in [
        "node --test sample.test.cjs",
        "cargo test --offline --manifest-path rust/Cargo.toml",
    ] {
        let mut check = request(temp.path(), command, CancellationToken::new());
        check.timeout_secs = 30;
        let result = runner.execute(check).await.unwrap();
        assert_eq!(result.exit_code, Some(0), "{command}: {result:?}");
    }
}
