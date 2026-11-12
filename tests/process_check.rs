use arc_mcp::{
    evidence::{CheckExecutor, CheckRequest},
    process_check::ProcessCheckExecutor,
};
#[cfg(not(target_os = "macos"))]
use std::time::Duration;
use std::{fs, path::Path, sync::Arc};
use tokio_util::sync::CancellationToken;

fn executor() -> Arc<ProcessCheckExecutor> {
    Arc::new(ProcessCheckExecutor::new(
        env!("CARGO_BIN_EXE_arc-mcp").into(),
    ))
}
fn request(root: &Path, command: &str, token: CancellationToken) -> CheckRequest {
    CheckRequest {
        project_root: root.into(),
        command: command.into(),
        timeout_secs: 30,
        max_output_bytes: 4096,
        cancellation: token,
    }
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn macos_refuses_native_execution_before_launch() {
    let temp = tempfile::tempdir().unwrap();
    let result = executor()
        .execute(request(
            temp.path(),
            "printf unguarded > unexpected.txt",
            CancellationToken::new(),
        ))
        .await;
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("macOS requires Seatbelt")
    );
    assert!(!temp.path().join("unexpected.txt").exists());

    let project = temp.path().join("project");
    fs::create_dir(&project).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_arc-mcp"))
        .arg("--root")
        .arg(&project)
        .arg("--data-home")
        .arg(temp.path().join("state"))
        .args(["--runner", "native", "serve"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("macOS MCP checks require Seatbelt"));
}

#[cfg(not(target_os = "macos"))]
#[tokio::test]
async fn native_output_exit_unicode_and_scope_are_honest() {
    let temp = tempfile::tempdir().unwrap();
    let command = if cfg!(windows) {
        "[Console]::Write('héllo'); [Console]::Error.Write('error'); exit 7"
    } else {
        "printf 'héllo'; printf error >&2; exit 7"
    };
    let result = executor()
        .execute(request(temp.path(), command, CancellationToken::new()))
        .await
        .unwrap();
    assert_eq!(result.exit_code, Some(7));
    assert_eq!(result.stdout, "héllo");
    assert_eq!(result.stderr, "error");
    assert!(result.runner.contains("NO filesystem/network sandbox"));
    assert!(!result.cancelled && !result.timed_out);
}

#[cfg(not(target_os = "macos"))]
#[tokio::test]
async fn native_cancellation_cleans_owned_background_children() {
    let temp = tempfile::tempdir().unwrap();
    let command = if cfg!(windows) {
        "Start-Process powershell.exe -NoNewWindow -ArgumentList '-NoProfile','-NonInteractive','-Command','Start-Sleep -Seconds 2; Set-Content -LiteralPath late.txt -Value leaked' -WorkingDirectory $PWD.Path; Set-Content started.txt ready; Start-Sleep -Seconds 20"
    } else {
        "(sleep 2; printf leaked > late.txt) & printf ready > started.txt; sleep 20"
    };
    let token = CancellationToken::new();
    let input = request(temp.path(), command, token.clone());
    let run = tokio::spawn(async move { executor().execute(input).await });
    let started = tokio::time::timeout(Duration::from_secs(20), async {
        while !temp.path().join("started.txt").exists() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await;
    if started.is_err() {
        token.cancel();
        panic!(
            "native command never reached its readiness marker: {:?}",
            run.await.unwrap()
        );
    }
    token.cancel();
    let result = run.await.unwrap().unwrap();
    assert!(result.cancelled);
    tokio::time::sleep(Duration::from_millis(2300)).await;
    assert!(
        !temp.path().join("late.txt").exists(),
        "owned background child survived cancellation"
    );
}

#[cfg(not(target_os = "macos"))]
#[tokio::test]
async fn native_timeout_precancel_and_output_cap() {
    let temp = tempfile::tempdir().unwrap();
    let token = CancellationToken::new();
    token.cancel();
    let write = if cfg!(windows) {
        "Set-Content unexpected.txt bad"
    } else {
        "printf bad > unexpected.txt"
    };
    assert!(
        executor()
            .execute(request(temp.path(), write, token))
            .await
            .is_err()
    );
    assert!(!temp.path().join("unexpected.txt").exists());
    let mut input = request(
        temp.path(),
        if cfg!(windows) {
            "Start-Sleep -Seconds 20"
        } else {
            "sleep 20"
        },
        CancellationToken::new(),
    );
    input.timeout_secs = 1;
    assert!(executor().execute(input).await.unwrap().timed_out);
    let output = if cfg!(windows) {
        "[Console]::Write(('x' * 10000))"
    } else {
        "i=0; while [ $i -lt 10000 ]; do printf x; i=$((i+1)); done"
    };
    let result = executor()
        .execute(request(temp.path(), output, CancellationToken::new()))
        .await
        .unwrap();
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert!(result.truncated);
    assert!(result.stdout.len() <= 2048);
}

#[cfg(not(target_os = "macos"))]
#[tokio::test]
async fn native_mode_does_not_pretend_to_isolate_filesystem_writes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project");
    fs::create_dir(&root).unwrap();
    let command = if cfg!(windows) {
        "Set-Content -LiteralPath ../outside.txt -Value allowed"
    } else {
        "printf allowed > ../outside.txt"
    };
    let result = executor()
        .execute(request(&root, command, CancellationToken::new()))
        .await
        .unwrap();
    assert_eq!(result.exit_code, Some(0), "{result:?}");
    assert!(temp.path().join("outside.txt").exists());
    assert!(result.runner.contains("NO filesystem/network sandbox"));
}
