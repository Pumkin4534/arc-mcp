use anyhow::Result;
use arc_mcp::{
    evidence::{self, CheckExecutor, CheckOutcome, CheckRequest},
    workflow::{CheckRecovery, HostBinding, ReviewReceipt, WorkflowEngine},
};
use serde_json::{Value, json};
use std::{
    fs,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;

struct Fixture {
    _base: TempDir,
    root: std::path::PathBuf,
    home: std::path::PathBuf,
    engine: WorkflowEngine,
}

impl Fixture {
    fn new() -> Self {
        let base = tempfile::tempdir().unwrap();
        let root = base.path().join("project");
        let home = base.path().join("private-state");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("source.rs"), "fn main() {}\n").unwrap();
        let engine = WorkflowEngine::new(&root, &home).unwrap();
        Self {
            _base: base,
            root,
            home,
            engine,
        }
    }
    fn with_outcome(mut self, exit: Option<i32>) -> Self {
        self.engine = self.engine.with_executor(Arc::new(FixtureExecutor {
            exit,
            mutate: false,
            calls: Arc::new(AtomicUsize::new(0)),
        }));
        self
    }
}

// Test-only trusted injection: never exposed as an MCP tool and never starts a shell.
struct FixtureExecutor {
    exit: Option<i32>,
    mutate: bool,
    calls: Arc<AtomicUsize>,
}
impl CheckExecutor for FixtureExecutor {
    fn execute(
        &self,
        request: CheckRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CheckOutcome>> + Send + '_>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.mutate {
                fs::write(request.project_root.join("source.rs"), "changed by fixture")?;
            }
            Ok(CheckOutcome {
                exit_code: self.exit,
                stdout: "fixture assertion output".into(),
                stderr: String::new(),
                timed_out: false,
                cancelled: false,
                truncated: false,
                runner: "test-only injected outcome; no shell".into(),
            })
        })
    }
}

async fn call(engine: &WorkflowEngine, name: &str, args: Value) -> Result<Value> {
    engine.call_tool(name, args, CancellationToken::new()).await
}

fn task(id: &str, deps: &[&str]) -> Value {
    json!({"id":id,"title":format!("Deliver {id}"),"acceptance":["The requested behavior is implemented and exercised by meaningful checks."],"depends_on":deps})
}

async fn plan(engine: &WorkflowEngine) -> Value {
    call(engine, "arc_plan", json!({"goal":"Implement the operator's requested behavior", "tasks":[task("deliver", &[])]})).await.unwrap()
}

async fn check(engine: &WorkflowEngine) -> Value {
    call(
        engine,
        "arc_run_check",
        json!({"task_ids":["deliver"],"command":"fixture-check","timeout_secs":5}),
    )
    .await
    .unwrap()
}

async fn ready(engine: &WorkflowEngine) {
    call(engine, "arc_progress", json!({"task_id":"deliver","status":"ready","reason":"Acceptance exercised by the linked check"})).await.unwrap();
}

fn bind(engine: &WorkflowEngine, plan: &Value) {
    engine
        .host_bind(HostBinding {
            workflow_id: plan["workflow_id"].as_str().unwrap().into(),
            owner_session_id: "actual-owner".into(),
            operator_goal: "Original operator message independently retrieved by host".into(),
        })
        .unwrap();
}

fn receipt(packet: &Value, role: &str, session: &str) -> ReviewReceipt {
    ReviewReceipt {
        workflow_id: packet["workflow_id"].as_str().unwrap().into(),
        packet_id: packet["packet_id"].as_str().unwrap().into(),
        owner_session_id: "actual-owner".into(),
        reviewer_session_id: session.into(),
        role: role.into(),
        verdict: "pass".into(),
        task_ids: serde_json::from_value(packet["task_ids"].clone()).unwrap(),
        evidence_ids: serde_json::from_value(packet["evidence_ids"].clone()).unwrap(),
        findings: vec![],
    }
}

#[tokio::test]
async fn plans_reject_invalid_scope_without_silent_truncation() {
    let f = Fixture::new();
    for tasks in [
        vec![],
        (0..6).map(|i| task(&format!("task{i}"), &[])).collect(),
        vec![task("first", &["later"]), task("later", &[])],
        vec![task("same", &[]), task("same", &[])],
    ] {
        assert!(
            call(
                &f.engine,
                "arc_plan",
                json!({"goal":"Keep all scope", "tasks":tasks})
            )
            .await
            .is_err()
        );
    }
    let mut oversized = task("all", &[]);
    oversized["acceptance"] = json!(["x".repeat(4001)]);
    assert!(
        call(
            &f.engine,
            "arc_plan",
            json!({"goal":"Keep scope", "tasks":[oversized]})
        )
        .await
        .is_err()
    );
    let full = (0..5)
        .map(|i| task(&format!("t{i}"), &[]))
        .collect::<Vec<_>>();
    let created = call(
        &f.engine,
        "arc_plan",
        json!({"goal":"All five outcomes", "tasks":full}),
    )
    .await
    .unwrap();
    assert_eq!(created["tasks"].as_array().unwrap().len(), 5);
    assert!(
        call(
            &f.engine,
            "arc_plan",
            json!({"goal":"Overwrite active plan", "tasks":[task("new", &[])]})
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("active_plan")
    );
    assert_eq!(
        f.engine.status().unwrap()["tasks"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
}

#[tokio::test]
async fn model_cannot_supply_roots_exit_codes_host_identity_or_review_verdicts() {
    let f = Fixture::new();
    assert!(
        call(
            &f.engine,
            "arc_plan",
            json!({"goal":"goal","tasks":[task("deliver", &[])],"root":"/tmp"})
        )
        .await
        .is_err()
    );
    plan(&f.engine).await;
    for name in [
        "host_bind",
        "host_accept_review",
        "arc_record_check",
        "arc_review",
        "host-review",
    ] {
        assert!(
            call(&f.engine, name, json!({"verdict":"pass"}))
                .await
                .unwrap_err()
                .to_string()
                .contains("unknown_tool")
        );
    }
    assert!(
        call(
            &f.engine,
            "arc_run_check",
            json!({"task_ids":["deliver"],"command":"check","timeout_secs":1,"exit_code":0})
        )
        .await
        .is_err()
    );
    assert!(
        call(
            &f.engine,
            "arc_finalize",
            json!({"reviews":[{"verdict":"pass"}]})
        )
        .await
        .is_err()
    );
    let status = f.engine.status().unwrap();
    assert_eq!(status["host_bound"], false);
    assert_eq!(status["complete"], false);
}

#[tokio::test]
async fn default_runner_fails_closed_and_persists_unknown_outcome() {
    let f = Fixture::new();
    plan(&f.engine).await;
    let output = check(&f.engine).await;
    assert_eq!(output["artifact"]["passed"], false);
    assert!(output["artifact"]["outcome"].is_null());
    assert!(
        output["artifact"]["error"]
            .as_str()
            .unwrap()
            .contains("guard_unconfigured")
    );
    assert!(
        call(
            &f.engine,
            "arc_progress",
            json!({"task_id":"deliver","status":"ready","reason":"I say it passed"})
        )
        .await
        .is_err()
    );
    assert_eq!(
        call(&f.engine, "arc_finalize", json!({})).await.unwrap()["complete"],
        false
    );
}

#[tokio::test]
async fn completion_requires_actual_binding_two_distinct_host_reviews_and_fresh_source() {
    let f = Fixture::new().with_outcome(Some(0));
    let created = plan(&f.engine).await;
    assert_eq!(check(&f.engine).await["artifact"]["passed"], true);
    ready(&f.engine).await;
    assert!(
        call(&f.engine, "arc_prepare_review", json!({}))
            .await
            .unwrap_err()
            .to_string()
            .contains("host_binding_required")
    );
    bind(&f.engine, &created);
    let packet = call(&f.engine, "arc_prepare_review", json!({}))
        .await
        .unwrap();
    let same = call(&f.engine, "arc_prepare_review", json!({}))
        .await
        .unwrap();
    assert_eq!(packet["packet_id"], same["packet_id"]);
    assert!(
        f.engine
            .host_accept_review(receipt(&packet, "correctness", "actual-owner"))
            .is_err()
    );
    let mut incomplete = receipt(&packet, "correctness", "review-a");
    incomplete.evidence_ids.clear();
    assert!(f.engine.host_accept_review(incomplete).is_err());
    f.engine
        .host_accept_review(receipt(&packet, "correctness", "review-a"))
        .unwrap();
    assert!(
        f.engine
            .host_accept_review(receipt(&packet, "adversarial", "review-a"))
            .is_err()
    );
    assert_eq!(
        call(&f.engine, "arc_finalize", json!({})).await.unwrap()["complete"],
        false
    );
    f.engine
        .host_accept_review(receipt(&packet, "adversarial", "review-b"))
        .unwrap();
    assert_eq!(f.engine.status().unwrap()["complete"], false);
    assert_eq!(
        call(&f.engine, "arc_finalize", json!({})).await.unwrap()["complete"],
        true
    );
    let reopened = WorkflowEngine::new(&f.root, &f.home).unwrap();
    assert_eq!(reopened.status().unwrap()["complete"], true);
    fs::write(f.root.join("source.rs"), "new source after review").unwrap();
    assert_eq!(reopened.status().unwrap()["complete"], false);
    let gate = call(&reopened, "arc_finalize", json!({})).await.unwrap();
    assert_eq!(gate["complete"], false);
    assert!(
        gate["next_action"]
            .as_str()
            .unwrap()
            .contains("stale_evidence")
    );
}

#[tokio::test]
async fn stale_source_rejects_review_before_finalization_and_revision_invalidates_checks() {
    let f = Fixture::new().with_outcome(Some(0));
    let created = plan(&f.engine).await;
    bind(&f.engine, &created);
    check(&f.engine).await;
    ready(&f.engine).await;
    let packet = call(&f.engine, "arc_prepare_review", json!({}))
        .await
        .unwrap();
    fs::write(f.root.join("source.rs"), "changed").unwrap();
    assert!(
        f.engine
            .host_accept_review(receipt(&packet, "correctness", "review-a"))
            .unwrap_err()
            .to_string()
            .contains("stale_evidence")
    );
    check(&f.engine).await;
    ready(&f.engine).await;
    let revised = call(&f.engine, "arc_revise_task", json!({"task_id":"deliver","title":"Broader deliverable","acceptance":["Original behavior plus the requested edge case"],"depends_on":[],"reason":"Operator clarified scope"})).await.unwrap();
    assert_eq!(revised["revision"], 2);
    assert!(
        call(
            &f.engine,
            "arc_progress",
            json!({"task_id":"deliver","status":"ready","reason":"Reuse old proof"})
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("missing_evidence")
    );
    assert_eq!(f.engine.status().unwrap()["packet_id"], Value::Null);
}

#[tokio::test]
async fn failed_latest_check_cannot_be_hidden_by_older_pass() {
    let f = Fixture::new().with_outcome(Some(0));
    plan(&f.engine).await;
    check(&f.engine).await;
    ready(&f.engine).await;
    let fail = WorkflowEngine::new(&f.root, &f.home)
        .unwrap()
        .with_executor(Arc::new(FixtureExecutor {
            exit: Some(2),
            mutate: false,
            calls: Arc::new(AtomicUsize::new(0)),
        }));
    let output = check(&fail).await;
    assert_eq!(output["artifact"]["outcome"]["exit_code"], 2);
    assert_eq!(output["artifact"]["passed"], false);
    assert!(
        call(
            &f.engine,
            "arc_progress",
            json!({"task_id":"deliver","status":"ready","reason":"The older check passed"})
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("failed_evidence")
    );
}

#[tokio::test]
async fn failed_reviewer_blocks_completion_with_actionable_findings() {
    let f = Fixture::new().with_outcome(Some(0));
    let created = plan(&f.engine).await;
    bind(&f.engine, &created);
    check(&f.engine).await;
    ready(&f.engine).await;
    let packet = call(&f.engine, "arc_prepare_review", json!({}))
        .await
        .unwrap();
    let mut failed = receipt(&packet, "correctness", "review-a");
    failed.verdict = "fail".into();
    failed.findings = vec!["Required edge case is not exercised".into()];
    f.engine.host_accept_review(failed).unwrap();
    f.engine
        .host_accept_review(receipt(&packet, "adversarial", "review-b"))
        .unwrap();
    assert!(
        f.engine
            .host_accept_review(receipt(&packet, "correctness", "review-c"))
            .unwrap_err()
            .to_string()
            .contains("duplicate_review")
    );
    let gate = call(&f.engine, "arc_finalize", json!({})).await.unwrap();
    assert_eq!(gate["complete"], false);
    assert!(
        gate["next_action"]
            .as_str()
            .unwrap()
            .contains("Required edge case")
    );
}

#[tokio::test]
async fn unknown_exit_and_source_mutation_never_become_passed_checks() {
    let f = Fixture::new().with_outcome(None);
    plan(&f.engine).await;
    assert_eq!(check(&f.engine).await["artifact"]["passed"], false);
    let mutate = WorkflowEngine::new(&f.root, &f.home)
        .unwrap()
        .with_executor(Arc::new(FixtureExecutor {
            exit: Some(0),
            mutate: true,
            calls: Arc::new(AtomicUsize::new(0)),
        }));
    let output = check(&mutate).await;
    assert_eq!(output["artifact"]["passed"], false);
    assert!(
        output["artifact"]["error"]
            .as_str()
            .unwrap()
            .contains("source_changed")
    );
}

#[tokio::test]
async fn dependency_order_is_enforced_and_blockers_are_reported() {
    let f = Fixture::new();
    call(&f.engine, "arc_plan", json!({"goal":"Ordered work","tasks":[task("foundation", &[]),task("delivery", &["foundation"])]})).await.unwrap();
    assert!(
        call(
            &f.engine,
            "arc_progress",
            json!({"task_id":"delivery","status":"working","reason":"Skip ahead"})
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("dependency_blocked")
    );
    call(&f.engine, "arc_progress", json!({"task_id":"foundation","status":"blocked","reason":"Operator must provide a missing specification"})).await.unwrap();
    let status = f.engine.status().unwrap();
    assert!(
        status["next_action"]
            .as_str()
            .unwrap()
            .contains("missing specification")
    );
}

#[test]
fn fingerprints_work_without_git_and_disclose_ignore_generated_secret_and_symlink_scope() {
    let f = Fixture::new();
    fs::write(f.root.join(".gitignore"), "ignored-data/\n").unwrap();
    for directory in ["ignored-data", "target", "node_modules"] {
        fs::create_dir(f.root.join(directory)).unwrap();
        fs::write(f.root.join(directory).join("excluded"), "old").unwrap();
    }
    fs::write(f.root.join(".env"), "SECRET=one").unwrap();
    let before = evidence::fingerprint(&f.root).unwrap();
    for directory in ["ignored-data", "target", "node_modules"] {
        fs::write(f.root.join(directory).join("excluded"), "changed").unwrap();
    }
    fs::write(f.root.join(".env"), "SECRET=two").unwrap();
    assert_eq!(
        before.sha256,
        evidence::fingerprint(&f.root).unwrap().sha256
    );
    assert!(!before.warnings.is_empty());
    fs::write(f.root.join("source.rs"), "changed").unwrap();
    assert_ne!(
        before.sha256,
        evidence::fingerprint(&f.root).unwrap().sha256
    );
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink("../private-state", f.root.join("state-link")).unwrap();
        let linked = evidence::fingerprint(&f.root).unwrap();
        assert!(
            linked
                .warnings
                .iter()
                .any(|w| w.contains("referent content"))
        );
    }
}

#[test]
fn state_cannot_live_inside_project_even_through_symlink_or_missing_directories() {
    let f = Fixture::new();
    assert!(WorkflowEngine::new(&f.root, f.root.join("new/private")).is_err());
    assert!(!f.root.join("new").exists());
    #[cfg(unix)]
    {
        let alias = f._base.path().join("alias");
        std::os::unix::fs::symlink(&f.root, &alias).unwrap();
        assert!(WorkflowEngine::new(&f.root, alias.join("new/private")).is_err());
        assert!(!f.root.join("new").exists());
    }
}

#[test]
fn source_over_thirty_megabytes_is_hashed_without_a_checkpoint_gate() {
    let f = Fixture::new();
    let file = fs::File::create(f.root.join("large-source.txt")).unwrap();
    file.set_len(32 * 1024 * 1024).unwrap();
    let fingerprint = evidence::fingerprint(&f.root).unwrap();
    assert!(fingerprint.bytes >= 32 * 1024 * 1024);
    assert_eq!(fingerprint.files, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn competing_engines_do_not_overwrite_an_active_plan() {
    let f = Fixture::new();
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let engine = WorkflowEngine::new(&f.root, &f.home).unwrap();
        tasks.push(tokio::spawn(async move {
            call(
                &engine,
                "arc_plan",
                json!({"goal":"Concurrent create", "tasks":[task("deliver", &[])]}),
            )
            .await
        }));
    }
    let mut succeeded = 0;
    for task in tasks {
        if task.await.unwrap().is_ok() {
            succeeded += 1;
        }
    }
    assert_eq!(succeeded, 1);
    assert_eq!(
        f.engine.status().unwrap()["tasks"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

struct CancelExecutor {
    entered: Arc<tokio::sync::Notify>,
    calls: Arc<AtomicUsize>,
}
impl CheckExecutor for CancelExecutor {
    fn execute(
        &self,
        request: CheckRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CheckOutcome>> + Send + '_>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.entered.notify_one();
            request.cancellation.cancelled().await;
            Ok(CheckOutcome {
                exit_code: None,
                stdout: String::new(),
                stderr: "cancelled fixture".into(),
                timed_out: false,
                cancelled: true,
                truncated: false,
                runner: "cancel fixture".into(),
            })
        })
    }
}

#[tokio::test]
async fn cancellation_is_persisted_and_never_replayed_and_claim_blocks_competing_checks() {
    let mut f = Fixture::new();
    let entered = Arc::new(tokio::sync::Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    f.engine = f.engine.with_executor(Arc::new(CancelExecutor {
        entered: entered.clone(),
        calls: calls.clone(),
    }));
    plan(&f.engine).await;
    let token = CancellationToken::new();
    let engine = f.engine.clone();
    let task_token = token.clone();
    let pending = tokio::spawn(async move {
        engine
            .call_tool(
                "arc_run_check",
                json!({"task_ids":["deliver"],"command":"wait-fixture","timeout_secs":30}),
                task_token,
            )
            .await
    });
    entered.notified().await;
    assert!(
        call(
            &f.engine,
            "arc_run_check",
            json!({"task_ids":["deliver"],"command":"second-check","timeout_secs":1})
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("check_in_progress")
    );
    token.cancel();
    let output = pending.await.unwrap().unwrap();
    assert_eq!(output["artifact"]["passed"], false);
    assert_eq!(output["artifact"]["outcome"]["cancelled"], true);
    assert!(f.engine.status().unwrap()["running_check"].is_null());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn missing_immutable_artifact_prevents_completion_despite_index() {
    let f = Fixture::new().with_outcome(Some(0));
    let created = plan(&f.engine).await;
    bind(&f.engine, &created);
    let result = check(&f.engine).await;
    ready(&f.engine).await;
    let storage = fs::read_dir(f.home.join("projects"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::remove_file(storage.join("artifacts").join(format!(
        "{}.json",
        result["artifact"]["id"].as_str().unwrap()
    )))
    .unwrap();
    assert!(
        call(&f.engine, "arc_prepare_review", json!({}))
            .await
            .unwrap_err()
            .to_string()
            .contains("missing_evidence")
    );
    assert_eq!(
        call(&f.engine, "arc_finalize", json!({})).await.unwrap()["complete"],
        false
    );
}

struct LargeOutputExecutor;
impl CheckExecutor for LargeOutputExecutor {
    fn execute(
        &self,
        _request: CheckRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CheckOutcome>> + Send + '_>> {
        Box::pin(async {
            Ok(CheckOutcome {
                exit_code: Some(0),
                stdout: "€".repeat(100_000),
                stderr: "😀".repeat(100_000),
                timed_out: false,
                cancelled: false,
                truncated: false,
                runner: "large-output fixture".into(),
            })
        })
    }
}

#[tokio::test]
async fn core_bounds_utf8_output_even_if_injected_executor_does_not() {
    let mut f = Fixture::new();
    f.engine = f.engine.with_executor(Arc::new(LargeOutputExecutor));
    plan(&f.engine).await;
    let result = check(&f.engine).await;
    let outcome = &result["artifact"]["outcome"];
    assert_eq!(outcome["truncated"], true);
    assert!(
        outcome["stdout"].as_str().unwrap().len() + outcome["stderr"].as_str().unwrap().len()
            <= evidence::MAX_OUTPUT_BYTES
    );
    assert_eq!(result["artifact"]["passed"], true); // Actual exit remains known; truncation is explicit.
}

#[tokio::test]
async fn timeout_cancels_executor_and_precancelled_request_has_no_side_effect() {
    let mut f = Fixture::new();
    let calls = Arc::new(AtomicUsize::new(0));
    f.engine = f.engine.with_executor(Arc::new(CancelExecutor {
        entered: Arc::new(tokio::sync::Notify::new()),
        calls: calls.clone(),
    }));
    plan(&f.engine).await;
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let args = json!({"task_ids":["deliver"],"command":"wait-fixture","timeout_secs":1});
    assert!(
        f.engine
            .call_tool("arc_run_check", args.clone(), cancelled)
            .await
            .is_err()
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        call(&f.engine, "arc_run_check", args),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result["artifact"]["passed"], false);
    assert_eq!(result["artifact"]["outcome"]["cancelled"], true);
    assert_eq!(result["artifact"]["outcome"]["timed_out"], true);
    assert!(
        result["artifact"]["error"]
            .as_str()
            .unwrap()
            .contains("timeout")
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn interrupted_claim_requires_exact_host_recovery_and_never_fabricates_exit_or_replays() {
    let mut f = Fixture::new();
    let entered = Arc::new(tokio::sync::Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    f.engine = f.engine.with_executor(Arc::new(CancelExecutor {
        entered: entered.clone(),
        calls: calls.clone(),
    }));
    let created = plan(&f.engine).await;
    let engine = f.engine.clone();
    let pending = tokio::spawn(async move {
        call(
            &engine,
            "arc_run_check",
            json!({"task_ids":["deliver"],"command":"wait-fixture","timeout_secs":30}),
        )
        .await
    });
    entered.notified().await;
    // Simulate loss of the owning task, with no process tree in this fixture.
    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    let state = f.engine.status().unwrap();
    let mut recovery = CheckRecovery {
        workflow_id: created["workflow_id"].as_str().unwrap().into(),
        check_id: "wrong-claim".into(),
        reason: "Fixture owner stopped; no child processes were started".into(),
    };
    assert!(f.engine.host_recover_check(recovery.clone()).is_err());
    recovery.check_id = state["running_check"]["id"].as_str().unwrap().into();
    assert!(
        call(
            &f.engine,
            "host_recover_check",
            serde_json::to_value(&recovery).unwrap()
        )
        .await
        .is_err()
    );
    assert_eq!(
        f.engine.host_recover_check(recovery).unwrap()["recovered"],
        true
    );
    let state = f.engine.status().unwrap();
    assert!(state["running_check"].is_null());
    assert_eq!(state["tasks"][0]["status"], "blocked");
    assert_eq!(state["complete"], false);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(
        call(
            &f.engine,
            "arc_progress",
            json!({"task_id":"deliver","status":"ready","reason":"Assume interrupted success"})
        )
        .await
        .is_err()
    );
}

// Re-enter this one test in separate OS processes. No shell or guarded checks run.
#[tokio::test]
async fn process_plan_contender() {
    let Ok(root) = std::env::var("ARC_CORE_TEST_ROOT") else {
        return;
    };
    let home = std::env::var("ARC_CORE_TEST_HOME").unwrap();
    let engine = WorkflowEngine::new(root, home).unwrap();
    let result = call(
        &engine,
        "arc_plan",
        json!({"goal":"Cross-process plan race", "tasks":[task("deliver", &[])]}),
    )
    .await;
    if let Err(error) = result {
        assert!(error.to_string().contains("active_plan"));
        std::process::exit(23);
    }
}

#[test]
fn cross_process_lock_serializes_competing_plan_creators() {
    let f = Fixture::new();
    let mut children = Vec::new();
    for _ in 0..4 {
        children.push(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "process_plan_contender"])
                .env("ARC_CORE_TEST_ROOT", &f.root)
                .env("ARC_CORE_TEST_HOME", &f.home)
                .stdout(std::process::Stdio::piped())
                .stderr(std::process::Stdio::piped())
                .spawn()
                .unwrap(),
        );
    }
    let mut successful = 0;
    for child in children {
        let output = child.wait_with_output().unwrap();
        match output.status.code() {
            Some(0) => successful += 1,
            Some(23) => {}
            other => panic!(
                "unexpected child exit {other:?}: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        }
    }
    assert_eq!(successful, 1);
    assert_eq!(
        f.engine.status().unwrap()["tasks"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
