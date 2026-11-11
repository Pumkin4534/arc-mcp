use anyhow::Result;
use arc_mcp::{
    evidence::{CheckExecutor, CheckOutcome, CheckRequest},
    protocol::{self, MAX_INPUT_BYTES},
    workflow::WorkflowEngine,
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
    time::Duration,
};
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, ReadHalf, WriteHalf};

struct Client {
    _temp: TempDir,
    reader: BufReader<ReadHalf<DuplexStream>>,
    writer: WriteHalf<DuplexStream>,
    server: tokio::task::JoinHandle<Result<()>>,
    engine: WorkflowEngine,
}

impl Client {
    fn new(executor: Option<Arc<dyn CheckExecutor>>) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("project");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("source.txt"), "source").unwrap();
        let mut engine = WorkflowEngine::new(root, temp.path().join("state")).unwrap();
        if let Some(executor) = executor {
            engine = engine.with_executor(executor);
        }
        let (client, server) = tokio::io::duplex(1024 * 1024);
        let (server_reader, server_writer) = tokio::io::split(server);
        let host = engine.clone();
        let server =
            tokio::spawn(async move { protocol::serve(server_reader, server_writer, host).await });
        let (reader, writer) = tokio::io::split(client);
        Self {
            _temp: temp,
            reader: BufReader::new(reader),
            writer,
            server,
            engine,
        }
    }
    async fn send(&mut self, value: Value) {
        let mut bytes = serde_json::to_vec(&value).unwrap();
        bytes.push(b'\n');
        self.writer.write_all(&bytes).await.unwrap();
    }
    async fn read(&mut self) -> Value {
        let mut line = String::new();
        let bytes = tokio::time::timeout(Duration::from_secs(10), self.reader.read_line(&mut line))
            .await
            .expect("protocol response timed out")
            .unwrap();
        assert!(bytes > 0, "server closed before response");
        serde_json::from_str(&line).expect("every stdout line must be JSON-RPC, never logs")
    }
    async fn request(&mut self, id: Value, method: &str, params: Value) -> Value {
        self.send(json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
            .await;
        let response = self.read().await;
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], id);
        response
    }
    async fn initialize(&mut self, version: &str) -> Value {
        let result = self.request(json!(1), "initialize", json!({"protocolVersion":version,"capabilities":{},"clientInfo":{"name":"fixture","version":"1"}})).await;
        self.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .await;
        result
    }
    async fn plan(&mut self) {
        let result = self.request(json!(3), "tools/call", json!({"name":"arc_plan","arguments":{"goal":"Exercise protocol checks","tasks":[{"id":"deliver","title":"Deliver behavior","acceptance":["Verify the expected behavior"],"depends_on":[]}]}})).await;
        assert_eq!(result["result"]["isError"], false);
    }
    async fn close(mut self) {
        self.writer.shutdown().await.unwrap();
        tokio::time::timeout(Duration::from_secs(10), self.server)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }
}

#[tokio::test]
async fn negotiates_modern_and_legacy_versions_and_lists_only_public_tools() {
    for version in ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"] {
        let mut client = Client::new(None);
        let initialized = client.initialize(version).await;
        assert_eq!(initialized["result"]["protocolVersion"], version);
        assert_eq!(
            initialized["result"]["capabilities"]["tools"]["listChanged"],
            false
        );
        let listed = client.request(json!(2), "tools/list", json!({})).await;
        let tools = listed["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 7);
        for tool in tools {
            let name = tool["name"].as_str().unwrap();
            assert!(
                !name.contains("host") && !name.contains("receipt") && !name.contains("record")
            );
            assert_eq!(tool["inputSchema"]["additionalProperties"], false);
        }
        let plan = tools.iter().find(|t| t["name"] == "arc_plan").unwrap();
        assert_eq!(plan["inputSchema"]["properties"]["tasks"]["maxItems"], 5);
        client.close().await;
    }
}

#[tokio::test]
async fn unknown_protocol_offers_supported_version_and_requires_initialized_notification() {
    let mut client = Client::new(None);
    assert_eq!(
        client.request(json!(1), "tools/list", json!({})).await["error"]["code"],
        -32002
    );
    assert_eq!(
        client.request(json!(2), "initialize", json!({})).await["error"]["code"],
        -32602
    );
    let result = client
        .request(
            json!(3),
            "initialize",
            json!({"protocolVersion":"2099-01-01"}),
        )
        .await;
    assert_eq!(result["result"]["protocolVersion"], "2025-11-25");
    assert_eq!(
        client.request(json!(4), "tools/list", json!({})).await["error"]["code"],
        -32002
    );
    client
        .send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
        .await;
    assert!(client.request(json!(5), "tools/list", json!({})).await["result"]["tools"].is_array());
    assert_eq!(
        client
            .request(
                json!(6),
                "initialize",
                json!({"protocolVersion":"2025-11-25"})
            )
            .await["error"]["code"],
        -32600
    );
    client.close().await;
}

#[tokio::test]
async fn malformed_and_oversized_lines_resynchronize_without_stdout_noise() {
    let mut client = Client::new(None);
    client.writer.write_all(b"not-json\n").await.unwrap();
    assert_eq!(client.read().await["error"]["code"], -32700);
    client.writer.write_all(b"[]\n").await.unwrap();
    assert_eq!(client.read().await["error"]["code"], -32600);
    client
        .writer
        .write_all(b"{\"jsonrpc\":\"1.0\",\"id\":3,\"method\":\"ping\"}\n")
        .await
        .unwrap();
    assert_eq!(client.read().await["error"]["code"], -32600);
    client
        .writer
        .write_all(&vec![b'x'; MAX_INPUT_BYTES + 64])
        .await
        .unwrap();
    client.writer.write_all(b"\n").await.unwrap();
    assert_eq!(client.read().await["error"]["code"], -32600);
    client.writer.write_all(&[0xff, b'\n']).await.unwrap();
    assert_eq!(client.read().await["error"]["code"], -32700);
    assert_eq!(
        client.request(json!("healthy"), "ping", json!({})).await["result"],
        json!({})
    );
    client.close().await;
}

#[tokio::test]
async fn notifications_never_receive_responses_and_invalid_ids_params_are_errors() {
    let mut client = Client::new(None);
    client.initialize("2025-11-25").await;
    for method in ["unknown/notification", "ping", "notifications/cancelled"] {
        client
            .send(json!({"jsonrpc":"2.0","method":method,"params":{"requestId":"not-running"}}))
            .await;
    }
    assert_eq!(
        client
            .request(json!("after-notifications"), "ping", json!({}))
            .await["result"],
        json!({})
    );
    client
        .send(json!({"jsonrpc":"2.0","id":{},"method":"ping"}))
        .await;
    let invalid = client.read().await;
    assert_eq!(invalid["id"], Value::Null);
    assert_eq!(invalid["error"]["code"], -32600);
    assert_eq!(
        client.request(json!(9), "tools/call", json!([])).await["error"]["code"],
        -32602
    );
    assert_eq!(
        client.request(json!(10), "nonexistent", json!({})).await["error"]["code"],
        -32601
    );
    client.close().await;
}

#[tokio::test]
async fn domain_failures_are_tool_errors_and_host_receipts_cannot_be_called() {
    let mut client = Client::new(None);
    client.initialize("2025-06-18").await;
    let bad = client
        .request(
            json!(2),
            "tools/call",
            json!({"name":"arc_plan","arguments":{"goal":"too many","tasks":[]}}),
        )
        .await;
    assert_eq!(bad["result"]["isError"], true);
    assert!(
        bad["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .contains("invalid_plan")
    );
    client.plan().await;
    let host = client
        .request(
            json!(4),
            "tools/call",
            json!({"name":"host_accept_review","arguments":{"verdict":"pass"}}),
        )
        .await;
    assert_eq!(host["error"]["code"], -32602);
    let forged = client
        .request(
            json!(5),
            "tools/call",
            json!({"name":"arc_finalize","arguments":{"verdict":"pass"}}),
        )
        .await;
    assert_eq!(forged["result"]["isError"], true);
    let incomplete = client
        .request(json!(6), "tools/call", json!({"name":"arc_finalize"}))
        .await;
    assert_eq!(incomplete["result"]["structuredContent"]["complete"], false);
    client.close().await;
}

struct WaitExecutor {
    entered: Arc<tokio::sync::Notify>,
    calls: Arc<AtomicUsize>,
}
impl CheckExecutor for WaitExecutor {
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
                stderr: String::new(),
                timed_out: false,
                cancelled: true,
                truncated: false,
                runner: "protocol cancel fixture (no shell)".into(),
            })
        })
    }
}

#[tokio::test]
async fn cancellation_reaches_owned_check_while_ping_remains_responsive_and_no_replay_occurs() {
    let entered = Arc::new(tokio::sync::Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let mut client = Client::new(Some(Arc::new(WaitExecutor {
        entered: entered.clone(),
        calls: calls.clone(),
    })));
    client.initialize("2025-11-25").await;
    client.plan().await;
    client.send(json!({"jsonrpc":"2.0","id":"check","method":"tools/call","params":{"name":"arc_run_check","arguments":{"task_ids":["deliver"],"command":"wait-fixture","timeout_secs":30}}})).await;
    tokio::time::timeout(Duration::from_secs(10), entered.notified())
        .await
        .unwrap();
    assert_eq!(
        client
            .request(json!("ping-during-check"), "ping", json!({}))
            .await["result"],
        json!({})
    );
    client.send(json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"check","reason":"Operator stopped"}})).await;
    let cancelled = client.read().await;
    assert_eq!(cancelled["id"], "check");
    assert_eq!(cancelled["error"]["code"], -32800);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let status = client
        .request(json!(7), "tools/call", json!({"name":"arc_status"}))
        .await;
    assert_eq!(status["result"]["structuredContent"]["complete"], false);
    assert!(status["result"]["structuredContent"]["running_check"].is_null());
    client.close().await;
}

#[tokio::test]
async fn input_disconnect_cancels_check_and_waits_for_recorded_outcome() {
    let entered = Arc::new(tokio::sync::Notify::new());
    let calls = Arc::new(AtomicUsize::new(0));
    let mut client = Client::new(Some(Arc::new(WaitExecutor {
        entered: entered.clone(),
        calls: calls.clone(),
    })));
    client.initialize("2025-11-25").await;
    client.plan().await;
    client.send(json!({"jsonrpc":"2.0","id":99,"method":"tools/call","params":{"name":"arc_run_check","arguments":{"task_ids":["deliver"],"command":"wait-fixture","timeout_secs":30}}})).await;
    tokio::time::timeout(Duration::from_secs(10), entered.notified())
        .await
        .unwrap();
    let engine = client.engine.clone();
    // Keep TempDir alive while inspecting persisted state after connection shutdown.
    client.writer.shutdown().await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), &mut client.server)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let status = engine.status().unwrap();
    assert!(status["running_check"].is_null());
    assert_eq!(status["complete"], false);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
