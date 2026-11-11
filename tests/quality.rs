//! Independent boundary review. All executors below are labelled fixtures: no
//! provider, shell, installed configuration, or non-disposable project is used.
use anyhow::{Result, bail};
use arc_mcp::{
    evidence::{CheckExecutor, CheckOutcome, CheckRequest},
    workflow::{HostBinding, ReviewReceipt, WorkflowEngine},
};
use serde_json::{Value, json};
use std::{
    fs,
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tempfile::TempDir;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

const OWNER: &str = "quality-fixture-owner";
const OPERATOR_GOAL: &str = "Deliver all five outcomes, including rollback, accessibility, and the final acceptance detail. Never replace this original request with a writer summary.";

struct Project {
    _temporary: TempDir,
    root: PathBuf,
    home: PathBuf,
    engine: WorkflowEngine,
}

impl Project {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path().join("disposable-project");
        let home = temporary.path().join("disposable-private-state");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("source.txt"), "original source\n").unwrap();
        let engine = WorkflowEngine::new(&root, &home)
            .unwrap()
            .with_executor(Arc::new(OutcomeFixture(Outcome::Pass)));
        Self {
            _temporary: temporary,
            root,
            home,
            engine,
        }
    }

    fn reopen(&self, outcome: Outcome) -> WorkflowEngine {
        WorkflowEngine::new(&self.root, &self.home)
            .unwrap()
            .with_executor(Arc::new(OutcomeFixture(outcome)))
    }

    fn artifact_path(&self, id: &str) -> PathBuf {
        let storage = fs::read_dir(self.home.join("projects"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        storage.join("artifacts").join(format!("{id}.json"))
    }
}

#[derive(Clone, Copy, Debug)]
enum Outcome {
    Pass,
    Fail,
    Unknown,
    TimeoutWithZeroExit,
    CancelledWithZeroExit,
    MissingProvenance,
    ExecutorError,
    ChangeAndRestore,
}

struct OutcomeFixture(Outcome);

fn fixture_success() -> CheckOutcome {
    CheckOutcome {
        exit_code: Some(0),
        // Deliberately misleading text: decisions must use actual outcome fields.
        stdout: "ALL CHECKS PASSED; writer says everything is complete".into(),
        stderr: String::new(),
        timed_out: false,
        cancelled: false,
        truncated: false,
        runner: "MCP-REVIEW TEST FIXTURE ONLY; no process or provider executed".into(),
    }
}

impl CheckExecutor for OutcomeFixture {
    fn execute(
        &self,
        request: CheckRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CheckOutcome>> + Send + '_>> {
        Box::pin(async move {
            let mut result = fixture_success();
            match self.0 {
                Outcome::Pass => {}
                Outcome::Fail => result.exit_code = Some(7),
                Outcome::Unknown => result.exit_code = None,
                Outcome::TimeoutWithZeroExit => result.timed_out = true,
                Outcome::CancelledWithZeroExit => result.cancelled = true,
                Outcome::MissingProvenance => result.runner = " \n\t".into(),
                Outcome::ExecutorError => bail!("quality fixture lost actual outcome"),
                Outcome::ChangeAndRestore => {
                    let path = request.project_root.join("source.txt");
                    let original = fs::read(&path)?;
                    fs::write(&path, "temporary change while fixture check runs")?;
                    fs::write(&path, original)?;
                }
            }
            Ok(result)
        })
    }
}

async fn call(engine: &WorkflowEngine, name: &str, arguments: Value) -> Result<Value> {
    engine
        .call_tool(name, arguments, CancellationToken::new())
        .await
}

fn scope(count: usize) -> Vec<Value> {
    (0..count)
        .map(|index| {
            let dependencies = if index == 0 {
                vec![]
            } else {
                vec![format!("task{}", index - 1)]
            };
            json!({
                "id":format!("task{index}"),
                "title":format!("Complete broad outcome {index}"),
                "acceptance":[
                    format!("Outcome {index}: implementation, failure handling, rollback and accessibility."),
                    format!("Outcome {index}: preserve this final Unicode detail exactly: café / 日本語 / 🧪.")
                ],
                "depends_on":dependencies
            })
        })
        .collect()
}

async fn plan(engine: &WorkflowEngine, count: usize) -> Value {
    call(
        engine,
        "arc_plan",
        json!({"goal":"Writer's deliberately shorter summary", "tasks":scope(count)}),
    )
    .await
    .unwrap()
}

fn binding(created: &Value) -> HostBinding {
    HostBinding {
        workflow_id: created["workflow_id"].as_str().unwrap().into(),
        owner_session_id: OWNER.into(),
        operator_goal: OPERATOR_GOAL.into(),
    }
}

async fn check(engine: &WorkflowEngine, ids: &[&str]) -> Value {
    call(
        engine,
        "arc_run_check",
        json!({"task_ids":ids,"command":"quality-fixture-only","timeout_secs":5}),
    )
    .await
    .unwrap()
}

async fn progress(engine: &WorkflowEngine, id: &str, status: &str) -> Result<Value> {
    call(
        engine,
        "arc_progress",
        json!({"task_id":id,"status":status,"reason":"Explicit fixture progress, not a review attestation"}),
    )
    .await
}

async fn packet(engine: &WorkflowEngine) -> Value {
    call(engine, "arc_prepare_review", json!({})).await.unwrap()
}

fn receipt(packet: &Value, role: &str, reviewer: &str) -> ReviewReceipt {
    ReviewReceipt {
        workflow_id: packet["workflow_id"].as_str().unwrap().into(),
        packet_id: packet["packet_id"].as_str().unwrap().into(),
        owner_session_id: OWNER.into(),
        reviewer_session_id: reviewer.into(),
        role: role.into(),
        verdict: "pass".into(),
        task_ids: serde_json::from_value(packet["task_ids"].clone()).unwrap(),
        evidence_ids: serde_json::from_value(packet["evidence_ids"].clone()).unwrap(),
        findings: vec![],
    }
}

fn reviews(engine: &WorkflowEngine, packet: &Value, suffix: &str) {
    for role in ["correctness", "adversarial"] {
        engine
            .host_accept_review(receipt(packet, role, &format!("fixture-{role}-{suffix}")))
            .unwrap();
    }
}

async fn ready_packet(project: &Project) -> Value {
    let created = plan(&project.engine, 1).await;
    project.engine.host_bind(binding(&created)).unwrap();
    check(&project.engine, &["task0"]).await;
    progress(&project.engine, "task0", "ready").await.unwrap();
    packet(&project.engine).await
}

async fn assert_incomplete(engine: &WorkflowEngine) {
    assert_eq!(engine.status().unwrap()["complete"], false);
    assert_eq!(
        call(engine, "arc_finalize", json!({})).await.unwrap()["complete"],
        false
    );
}

#[tokio::test]
async fn five_ordered_tasks_preserve_every_detail_through_restart_and_review_packet() {
    let project = Project::new();
    let created = plan(&project.engine, 5).await;
    project.engine.host_bind(binding(&created)).unwrap();
    let reopened = project.reopen(Outcome::Pass);
    let before = reopened.status().unwrap();
    for (index, expected) in scope(5).iter().enumerate() {
        for field in ["id", "title", "acceptance", "depends_on"] {
            assert_eq!(before["tasks"][index][field], expected[field]);
        }
    }
    assert!(progress(&reopened, "task4", "working").await.is_err());
    check(&reopened, &["task0", "task1", "task2", "task3", "task4"]).await;
    for index in 0..5 {
        progress(&reopened, &format!("task{index}"), "ready")
            .await
            .unwrap();
    }
    let proof = packet(&reopened).await;
    assert_eq!(proof["operator_goal"], OPERATOR_GOAL);
    assert_ne!(proof["operator_goal"], proof["plan_goal"]);
    assert_eq!(proof["task_ids"].as_array().unwrap().len(), 5);
    assert_eq!(proof["evidence_ids"].as_array().unwrap().len(), 1);
    for (index, expected) in scope(5).iter().enumerate() {
        for field in ["id", "title", "acceptance", "depends_on"] {
            assert_eq!(proof["tasks"][index][field], expected[field]);
        }
    }
    reviews(&reopened, &proof, "full-scope");
    assert_eq!(
        call(&reopened, "arc_finalize", json!({})).await.unwrap()["complete"],
        true
    );
}

#[tokio::test]
async fn invalid_revision_and_stale_completed_plan_cannot_replace_existing_scope() {
    let project = Project::new();
    let proof = ready_packet(&project).await;
    reviews(&project.engine, &proof, "original");
    call(&project.engine, "arc_finalize", json!({}))
        .await
        .unwrap();
    let before = project.engine.status().unwrap();
    for acceptance in [json!([]), json!(["x".repeat(4001)])] {
        assert!(
            call(
                &project.engine,
                "arc_revise_task",
                json!({
                    "task_id":"task0", "title":"Attempt to replace complete scope",
                    "acceptance":acceptance,"depends_on":[],"reason":"Invalid fixture revision"
                })
            )
            .await
            .is_err()
        );
        let after = project.engine.status().unwrap();
        assert_eq!(after["tasks"], before["tasks"]);
        assert_eq!(after["revision"], before["revision"]);
        assert_eq!(after["packet_id"], before["packet_id"]);
    }
    fs::write(project.root.join("source.txt"), "unverified follow-up").unwrap();
    assert!(
        call(
            &project.engine,
            "arc_plan",
            json!({"goal":"Discard stale completion","tasks":scope(1)})
        )
        .await
        .is_err()
    );
    let after = project.engine.status().unwrap();
    assert_eq!(after["workflow_id"], before["workflow_id"]);
    // Evidence freshness legitimately changes; scope does not.
    for field in ["id", "title", "acceptance", "depends_on"] {
        assert_eq!(after["tasks"][0][field], before["tasks"][0][field]);
    }
    assert_incomplete(&project.engine).await;
}

#[tokio::test]
async fn actual_failure_flags_override_success_text_and_previous_fully_reviewed_pass() {
    for outcome in [
        Outcome::Fail,
        Outcome::Unknown,
        Outcome::TimeoutWithZeroExit,
        Outcome::CancelledWithZeroExit,
        Outcome::MissingProvenance,
        Outcome::ExecutorError,
    ] {
        let project = Project::new();
        let proof = ready_packet(&project).await;
        reviews(&project.engine, &proof, "prior-pass");
        assert_eq!(
            call(&project.engine, "arc_finalize", json!({}))
                .await
                .unwrap()["complete"],
            true
        );
        let failing = project.reopen(outcome);
        let result = check(&failing, &["task0"]).await;
        assert_eq!(result["artifact"]["passed"], false, "{outcome:?}");
        assert!(
            progress(&failing, "task0", "ready").await.is_err(),
            "{outcome:?}"
        );
        assert!(
            call(&failing, "arc_prepare_review", json!({}))
                .await
                .is_err()
        );
        assert!(
            failing
                .host_accept_review(receipt(&proof, "correctness", "new-fixture-reviewer"))
                .is_err()
        );
        assert_incomplete(&project.reopen(Outcome::Pass)).await;
    }
}

#[tokio::test]
async fn latest_per_task_evidence_cannot_hide_a_failed_subset_behind_a_shared_pass() {
    let project = Project::new();
    let created = plan(&project.engine, 2).await;
    project.engine.host_bind(binding(&created)).unwrap();
    let shared = check(&project.engine, &["task0", "task1"]).await;
    for id in ["task0", "task1"] {
        progress(&project.engine, id, "ready").await.unwrap();
    }
    let failing = project.reopen(Outcome::Fail);
    check(&failing, &["task1"]).await;
    assert!(packet_result(&failing).await.is_err());
    let state = failing.status().unwrap();
    assert!(state["tasks"][0]["evidence_issue"].is_null());
    assert!(
        state["tasks"][1]["evidence_issue"]
            .as_str()
            .unwrap()
            .contains("failed_evidence")
    );
    let repaired = check(&project.engine, &["task1"]).await;
    progress(&project.engine, "task1", "ready").await.unwrap();
    let proof = packet(&project.engine).await;
    assert_eq!(
        proof["evidence_ids"],
        json!([shared["artifact"]["id"], repaired["artifact"]["id"]])
    );
    let mut partial = receipt(&proof, "correctness", "partial-review");
    partial.evidence_ids.pop();
    assert!(project.engine.host_accept_review(partial).is_err());
    reviews(&project.engine, &proof, "subset-repaired");
    assert_eq!(
        call(&project.engine, "arc_finalize", json!({}))
            .await
            .unwrap()["complete"],
        true
    );
}

async fn packet_result(engine: &WorkflowEngine) -> Result<Value> {
    call(engine, "arc_prepare_review", json!({})).await
}

#[tokio::test]
async fn binding_is_immutable_and_receipts_cannot_cross_owner_workflow_or_packet() {
    let project = Project::new();
    let created = plan(&project.engine, 1).await;
    let original = binding(&created);
    project.engine.host_bind(original.clone()).unwrap();
    project.engine.host_bind(original.clone()).unwrap();
    for field in ["workflow", "owner", "goal"] {
        let mut changed = original.clone();
        match field {
            "workflow" => changed.workflow_id = "another-workflow".into(),
            "owner" => changed.owner_session_id = "another-owner".into(),
            _ => changed.operator_goal = "Writer reduced original scope".into(),
        }
        assert!(project.engine.host_bind(changed).is_err());
    }
    check(&project.engine, &["task0"]).await;
    progress(&project.engine, "task0", "ready").await.unwrap();
    let proof = packet(&project.engine).await;
    for field in [
        "workflow",
        "owner",
        "packet",
        "duplicate-task",
        "extra-evidence",
        "unknown-verdict",
    ] {
        let mut invalid = receipt(&proof, "correctness", "not-yet-consumed-reviewer");
        match field {
            "workflow" => invalid.workflow_id = "another-workflow".into(),
            "owner" => invalid.owner_session_id = "another-owner".into(),
            "packet" => invalid.packet_id = "another-packet".into(),
            "duplicate-task" => invalid.task_ids.push(invalid.task_ids[0].clone()),
            "extra-evidence" => invalid.evidence_ids.push("invented-proof".into()),
            _ => invalid.verdict = "unknown".into(),
        }
        assert!(
            project.engine.host_accept_review(invalid).is_err(),
            "{field}"
        );
    }
    project
        .engine
        .host_accept_review(receipt(&proof, "correctness", "not-yet-consumed-reviewer"))
        .unwrap();
    assert_incomplete(&project.engine).await;
    project
        .engine
        .host_accept_review(receipt(&proof, "adversarial", "distinct-fixture-reviewer"))
        .unwrap();
    assert_eq!(
        call(&project.engine, "arc_finalize", json!({}))
            .await
            .unwrap()["complete"],
        true
    );
    assert_eq!(
        project.engine.status().unwrap()["operator_goal"],
        OPERATOR_GOAL
    );
}

#[tokio::test]
async fn used_reviewer_identity_cannot_be_recycled_after_a_real_scope_revision() {
    let project = Project::new();
    let old = ready_packet(&project).await;
    reviews(&project.engine, &old, "used");
    call(&project.engine, "arc_revise_task", json!({
        "task_id":"task0","title":"Broader outcome", "acceptance":["Original behavior plus a newly requested edge case"],
        "depends_on":[],"reason":"Fixture operator clarifies scope"
    })).await.unwrap();
    check(&project.engine, &["task0"]).await;
    progress(&project.engine, "task0", "ready").await.unwrap();
    let new = packet(&project.engine).await;
    assert_ne!(old["packet_id"], new["packet_id"]);
    let reopened = project.reopen(Outcome::Pass);
    assert!(
        reopened
            .host_accept_review(receipt(&new, "correctness", "fixture-correctness-used"))
            .is_err()
    );
    assert!(
        reopened
            .host_accept_review(receipt(&old, "correctness", "unused-but-stale-packet"))
            .is_err()
    );
    reviews(&reopened, &new, "new-native-sessions");
    assert_eq!(
        call(&reopened, "arc_finalize", json!({})).await.unwrap()["complete"],
        true
    );
}

#[tokio::test]
async fn progress_only_cycles_must_not_erase_failed_findings_or_allow_verdict_shopping() {
    for cycle in [
        vec!["ready"],
        vec!["working", "ready"],
        vec!["blocked", "ready"],
    ] {
        let project = Project::new();
        let old = ready_packet(&project).await;
        let mut failed = receipt(&old, "correctness", "fixture-found-defect");
        failed.verdict = "fail".into();
        failed.findings = vec!["Required rollback behavior is missing".into()];
        project.engine.host_accept_review(failed).unwrap();
        for status in &cycle {
            // A refusal is also safe; the critical invariant is retaining the failed proof.
            let _ = progress(&project.engine, "task0", status).await;
        }
        if let Ok(new) = packet_result(&project.engine).await {
            assert_eq!(new["evidence_ids"], old["evidence_ids"]);
            assert!(
                project
                    .engine
                    .host_accept_review(receipt(
                        &new,
                        "correctness",
                        "fixture-shopping-replacement"
                    ))
                    .is_err(),
                "Q1: {cycle:?} erased a failed verdict with identical scope/source/check evidence"
            );
        }
        assert_incomplete(&project.engine).await;
        assert!(
            project.engine.status().unwrap()["review_roles"]
                .as_array()
                .unwrap()
                .iter()
                .any(|role| role["verdict"] == "fail")
        );
    }
}

#[tokio::test]
async fn rename_deletion_and_executable_mode_changes_after_reviews_invalidate_proof() {
    for change in ["rename", "delete", "executable"] {
        let project = Project::new();
        let proof = ready_packet(&project).await;
        reviews(&project.engine, &proof, change);
        let path = project.root.join("source.txt");
        match change {
            "rename" => fs::rename(&path, project.root.join("renamed.txt")).unwrap(),
            "delete" => fs::remove_file(&path).unwrap(),
            _ => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
                }
                #[cfg(not(unix))]
                continue;
            }
        }
        assert_incomplete(&project.reopen(Outcome::Pass)).await;
        assert!(packet_result(&project.engine).await.is_err());
    }
}

#[tokio::test]
async fn artifact_failure_is_revalidated_even_after_both_reviews_were_accepted() {
    let project = Project::new();
    let proof = ready_packet(&project).await;
    reviews(&project.engine, &proof, "before-damage");
    let path = project.artifact_path(proof["evidence_ids"][0].as_str().unwrap());
    let original: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    for outcome in [
        json!(null),
        json!({"exit_code":7,"stdout":"PASSED","stderr":"","timed_out":false,"cancelled":false,"truncated":false,"runner":"damaged fixture artifact"}),
    ] {
        // Simulates damaged/unprotected persisted evidence, never touches real state.
        let mut damaged = original.clone();
        damaged["outcome"] = outcome;
        assert_eq!(damaged["passed"], true); // The cached success bit must not be enough.
        fs::write(&path, serde_json::to_vec(&damaged).unwrap()).unwrap();
        assert_incomplete(&project.engine).await;
    }
}

struct CancelButClaimsSuccessFixture {
    entered: Arc<Notify>,
    calls: Arc<AtomicUsize>,
}

impl CheckExecutor for CancelButClaimsSuccessFixture {
    fn execute(
        &self,
        request: CheckRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CheckOutcome>> + Send + '_>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.entered.notify_one();
            request.cancellation.cancelled().await;
            // Core must still reject success when executor metadata omits cancellation.
            Ok(fixture_success())
        })
    }
}

#[tokio::test]
async fn active_claim_blocks_all_mutation_and_cancelled_success_never_replays_on_reopen() {
    let project = Project::new();
    let created = plan(&project.engine, 1).await;
    project.engine.host_bind(binding(&created)).unwrap();
    let entered = Arc::new(Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let waiting = project
        .engine
        .clone()
        .with_executor(Arc::new(CancelButClaimsSuccessFixture {
            entered: entered.clone(),
            calls: calls.clone(),
        }));
    let cancellation = CancellationToken::new();
    let token = cancellation.clone();
    let running = tokio::spawn(async move {
        waiting
            .call_tool(
                "arc_run_check",
                json!({"task_ids":["task0"],"command":"cancel-quality-fixture","timeout_secs":30}),
                token,
            )
            .await
    });
    tokio::time::timeout(Duration::from_secs(5), entered.notified())
        .await
        .unwrap();
    let reopened = project.reopen(Outcome::Pass);
    let claim = reopened.status().unwrap()["running_check"]["id"].clone();
    assert!(!claim.is_null());
    for (name, arguments) in [
        (
            "arc_plan",
            json!({"goal":"Replace active claim","tasks":scope(1)}),
        ),
        (
            "arc_revise_task",
            json!({"task_id":"task0","title":"Replace claim scope","acceptance":["Changed"],"depends_on":[],"reason":"Attempt revision while check active"}),
        ),
        (
            "arc_progress",
            json!({"task_id":"task0","status":"blocked","reason":"Do not erase active claim"}),
        ),
        (
            "arc_run_check",
            json!({"task_ids":["task0"],"command":"competing-fixture","timeout_secs":1}),
        ),
        ("arc_prepare_review", json!({})),
    ] {
        assert!(call(&reopened, name, arguments).await.is_err(), "{name}");
        assert_eq!(reopened.status().unwrap()["running_check"]["id"], claim);
    }
    assert!(reopened.host_bind(binding(&created)).is_err());
    assert_incomplete(&reopened).await;
    cancellation.cancel();
    let result = tokio::time::timeout(Duration::from_secs(5), running)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(result["artifact"]["outcome"]["exit_code"], 0);
    assert_eq!(result["artifact"]["passed"], false);
    assert!(
        result["artifact"]["error"]
            .as_str()
            .unwrap()
            .contains("cancelled")
    );
    for _ in 0..3 {
        let reopened = project.reopen(Outcome::Pass);
        assert!(reopened.status().unwrap()["running_check"].is_null());
        assert_incomplete(&reopened).await;
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn change_and_restore_is_explicitly_an_observation_limit_not_atomic_proof() {
    let project = Project::new();
    plan(&project.engine, 1).await;
    let result = check(&project.reopen(Outcome::ChangeAndRestore), &["task0"]).await;
    // Characterizes an acknowledged limit. It must remain visible in release claims.
    assert_eq!(result["artifact"]["passed"], true);
    assert!(
        result["artifact"]["before"]["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|warning| warning
                .as_str()
                .unwrap()
                .contains("not a filesystem snapshot"))
    );
    assert_incomplete(&project.engine).await;
}
