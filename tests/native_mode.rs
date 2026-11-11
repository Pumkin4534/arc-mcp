use arc_mcp::workflow::WorkflowEngine;
use serde_json::json;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn native_records_are_explicitly_advisory_and_cannot_open_protected_ledgers() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    std::fs::create_dir(&project).unwrap();
    let home = temp.path().join("state");
    let native = WorkflowEngine::new(&project, &home)
        .unwrap()
        .with_native_mode();
    let plan = native.call_tool("arc_plan", json!({"goal":"Native mode", "tasks":[{"id":"one","title":"Outcome", "acceptance":["Real evidence"], "depends_on":[]}]}), CancellationToken::new()).await.unwrap();
    assert_eq!(plan["assurance"], "native-advisory");
    assert_eq!(plan["tamper_protected"], false);
    assert_eq!(native.status().unwrap()["assurance"], "native-advisory");
    let protected = WorkflowEngine::new(&project, &home).unwrap();
    assert!(
        protected
            .status()
            .unwrap_err()
            .to_string()
            .contains("assurance_mismatch")
    );
    let failed = native
        .call_tool("arc_finalize", json!({}), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(failed["complete"], false);
    assert_eq!(failed["assurance"], "native-advisory");
}
