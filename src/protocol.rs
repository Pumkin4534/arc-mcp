//! Newline-delimited stdio JSON-RPC/MCP. All output on the supplied writer is protocol.
use crate::{VERSION, workflow::WorkflowEngine};
use anyhow::Result;
use serde_json::{Value, json};
use std::collections::HashMap;
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader},
    sync::mpsc,
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

pub const MAX_INPUT_BYTES: usize = 256 * 1024;
const MAX_IN_FLIGHT: usize = 8;
const SUPPORTED_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"];

fn object_schema(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}

pub fn tool_definitions() -> Value {
    let id = json!({"type":"string","minLength":1,"maxLength":64,"pattern":"^[A-Za-z0-9_-]+$"});
    let title = json!({"type":"string","minLength":1,"maxLength":500});
    let acceptance = json!({"type":"array","minItems":1,"maxItems":32,"items":{"type":"string","minLength":1,"maxLength":4000}});
    let dependencies = json!({"type":"array","maxItems":4,"uniqueItems":true,"items":id});
    let reason = json!({"type":"string","minLength":1,"maxLength":4000});
    let task = object_schema(
        json!({"id":id,"title":title,"acceptance":acceptance,"depends_on":dependencies}),
        &["id", "title", "acceptance", "depends_on"],
    );
    let empty = object_schema(json!({}), &[]);
    json!([
        {"name":"arc_plan","description":"Create 1–5 broad dependency-ordered tasks with full acceptance details. Rejects oversized or unfinished replacement plans; never truncates scope. Root is host-bound.","inputSchema":object_schema(json!({"goal":{"type":"string","minLength":1,"maxLength":16000},"tasks":{"type":"array","minItems":1,"maxItems":5,"items":task}}), &["goal","tasks"])},
        {"name":"arc_revise_task","description":"Explicitly revise task scope with a reason. Invalidates all prior evidence eligibility and independent receipts; preserves immutable artifacts.","inputSchema":object_schema(json!({"task_id":id,"title":title,"acceptance":acceptance,"depends_on":dependencies,"reason":reason}), &["task_id","title","acceptance","depends_on","reason"])},
        {"name":"arc_status","description":"Get compact workflow status, current-source freshness, blockers and the next actionable task. Ready does not mean independently verified.","inputSchema":empty,"annotations":{"readOnlyHint":true}},
        {"name":"arc_progress","description":"Mark working, ready or blocked with a reason. Declared dependencies must be ready and current; ready requires an actual successful current-scope check.","inputSchema":object_schema(json!({"task_id":id,"status":{"type":"string","enum":["working","ready","blocked"]},"reason":reason}), &["task_id","status","reason"])},
        {"name":"arc_run_check","description":"Execute one real check with the host-selected runner. macOS uses Seatbelt; native Windows (PowerShell) and Linux (POSIX shell) checks are NOT filesystem/network sandboxed. Captures bounded output/exit and source fingerprints. No result input. Never automatically retry after interruption.","inputSchema":object_schema(json!({"task_ids":{"type":"array","minItems":1,"maxItems":5,"uniqueItems":true,"items":id},"command":{"type":"string","minLength":1,"maxLength":16000},"timeout_secs":{"type":"integer","minimum":1,"maximum":600}}), &["task_ids","command","timeout_secs"]),"annotations":{"readOnlyHint":false,"destructiveHint":true,"idempotentHint":false}},
        {"name":"arc_prepare_review","description":"Prepare exact operator goal/task/evidence/source packet for two fresh native readonly reviewer roles. Requires host owner binding and all tasks ready with current successful evidence. This tool cannot accept review verdicts.","inputSchema":empty},
        {"name":"arc_finalize","description":"Request completion gate: both distinct native reviewer records and all current successful evidence must match current scope/source. Native-mode results are advisory, not tamper-proof attestations. Incomplete workflows return actionable blockers.","inputSchema":empty}
    ])
}

fn error(id: Value, code: i64, message: impl Into<String>) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message.into()}})
}

fn response(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}

fn tool_result(result: Result<Value>) -> Value {
    match result {
        Ok(value) => {
            json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":false})
        }
        Err(error) => {
            let message = format!("{error:#}");
            json!({"content":[{"type":"text","text":message}],"structuredContent":{"error":message,"complete":false},"isError":true})
        }
    }
}

enum Line {
    Data(Vec<u8>),
    Oversized,
    Io(String),
}

async fn read_lines<R: AsyncRead + Unpin>(reader: R, sender: mpsc::Sender<Line>) {
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let available = match reader.fill_buf().await {
            Ok(bytes) => bytes,
            Err(error) => {
                let _ = sender.send(Line::Io(error.to_string())).await;
                return;
            }
        };
        if available.is_empty() {
            if oversized {
                let _ = sender.send(Line::Oversized).await;
            } else if !line.is_empty() {
                let _ = sender.send(Line::Data(line)).await;
            }
            return;
        }
        let newline = available.iter().position(|b| *b == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        if !oversized {
            if line.len() + consumed > MAX_INPUT_BYTES {
                oversized = true;
                line.clear();
            } else {
                line.extend_from_slice(&available[..consumed]);
            }
        }
        reader.consume(consumed);
        if newline.is_some() {
            let message = if oversized {
                Line::Oversized
            } else {
                Line::Data(std::mem::take(&mut line))
            };
            if sender.send(message).await.is_err() {
                return;
            }
            oversized = false;
        }
    }
}

async fn write_message<W: AsyncWrite + Unpin>(writer: &mut W, value: &Value) -> Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn serve<R, W>(reader: R, mut writer: W, engine: WorkflowEngine) -> Result<()>
where
    R: AsyncRead + Unpin + Send + 'static,
    W: AsyncWrite + Unpin,
{
    let (sender, mut receiver) = mpsc::channel(8);
    let reader_task = tokio::spawn(read_lines(reader, sender));
    let mut initialized = false;
    let mut ready = false;
    let mut active: HashMap<String, CancellationToken> = HashMap::new();
    let mut tasks = JoinSet::new();
    let result = async {
        loop {
            tokio::select! {
                completed = tasks.join_next(), if !tasks.is_empty() => {
                    let (key, value) = completed.expect("nonempty task set")??;
                    active.remove(&key);
                    write_message(&mut writer, &value).await?;
                }
                line = receiver.recv() => {
                    let Some(line) = line else { break; };
                    let bytes = match line {
                        Line::Data(bytes) => bytes,
                        Line::Oversized => { write_message(&mut writer, &error(Value::Null, -32600, "request exceeds 256KiB line budget; discarded through newline")).await?; continue; }
                        Line::Io(message) => anyhow::bail!("stdio input failed: {message}"),
                    };
                    if bytes.iter().all(u8::is_ascii_whitespace) { continue; }
                    let value: Value = match serde_json::from_slice(&bytes) {
                        Ok(value) => value,
                        Err(_) => { write_message(&mut writer, &error(Value::Null, -32700, "Parse error: expected one JSON-RPC object per line")).await?; continue; }
                    };
                    let id = value.get("id").cloned();
                    let valid_id = id.as_ref().is_none_or(|id| id.is_null() || id.is_string() || id.as_i64().is_some() || id.as_u64().is_some());
                    if !value.is_object() || value["jsonrpc"] != "2.0" || !value["method"].is_string() || !valid_id {
                        write_message(&mut writer, &error(if valid_id { id.unwrap_or(Value::Null) } else { Value::Null }, -32600, "Invalid Request: JSON-RPC 2.0 object, method and scalar ID required; batches unsupported")).await?;
                        continue;
                    }
                    let method = value["method"].as_str().unwrap_or_default();
                    let params = value.get("params").cloned().unwrap_or_else(|| json!({}));
                    let Some(id) = id else {
                        match method {
                            "notifications/initialized" if initialized => ready = true,
                            "notifications/cancelled" => {
                                if let Some(id) = params.get("requestId")
                                    && let Some(token) = active.get(&id.to_string()) { token.cancel();
                                }
                            }
                            _ => {} // JSON-RPC notifications never receive responses.
                        }
                        continue;
                    };
                    let key = id.to_string();
                    if active.contains_key(&key) {
                        write_message(&mut writer, &error(id, -32600, "duplicate in-flight request ID")).await?;
                        continue;
                    }
                    if !params.is_object() {
                        write_message(&mut writer, &error(id, -32602, "Invalid params: expected object")).await?;
                        continue;
                    }
                    let immediate = match method {
                        "initialize" => {
                            if initialized { error(id, -32600, "already initialized") }
                            else if let Some(requested) = params["protocolVersion"].as_str() {
                                let negotiated = if SUPPORTED_VERSIONS.contains(&requested) { requested } else { "2025-11-25" };
                                initialized = true;
                                response(id, json!({"protocolVersion":negotiated,"capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"arc-mcp","version":VERSION},"instructions":"OpenCode owns models, UI and providers. Use 1–5 broad ordered tasks, actual checks, fresh native reviews through arc_review, and the completion gate. Check artifacts disclose the runner: macOS Seatbelt or native Windows/Linux execution without filesystem/network isolation. Records are native-advisory, not tamper-proof attestations. Scope/secret/generated exclusions are disclosed. Never invent proof or automatically replay checks after cancellation."}))
                            } else { error(id, -32602, "initialize requires protocolVersion string") }
                        }
                        "ping" => response(id, json!({})),
                        _ if !ready => error(id, -32002, "not initialized: send initialize and notifications/initialized first"),
                        "tools/list" => {
                            if params.get("cursor").is_some() { error(id, -32602, "no pagination cursor supported") }
                            else { response(id, json!({"tools":tool_definitions()})) }
                        }
                        "tools/call" => {
                            let Some(name) = params["name"].as_str() else {
                                write_message(&mut writer, &error(id, -32602, "tools/call requires name string")).await?;
                                continue;
                            };
                            let arguments = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
                            if !arguments.is_object() {
                                error(id, -32602, "tool arguments must be an object")
                            } else if !tool_definitions().as_array().is_some_and(|tools| tools.iter().any(|tool| tool["name"] == name)) {
                                error(id, -32602, format!("unknown tool: {name}"))
                            } else if active.len() >= MAX_IN_FLIGHT {
                                error(id, -32000, "busy: maximum 8 in-flight requests; wait before issuing more")
                            } else {
                                let token = CancellationToken::new();
                                active.insert(key.clone(), token.clone());
                                let engine = engine.clone();
                                let name = name.to_owned();
                                tasks.spawn(async move {
                                    let result = engine.call_tool(&name, arguments, token.clone()).await;
                                    let value = if token.is_cancelled() { error(id, -32800, "request cancelled; owned checks are not replayed; inspect arc_status") }
                                        else { response(id, tool_result(result)) };
                                    Ok::<_, anyhow::Error>((key, value))
                                });
                                continue;
                            }
                        }
                        _ => error(id, -32601, format!("Method not found: {method}")),
                    };
                    write_message(&mut writer, &immediate).await?;
                }
            }
        }
        Ok::<_, anyhow::Error>(())
    }.await;
    // EOF/disconnect/failed output stops owned work; wait for guarded child cleanup.
    for token in active.values() {
        token.cancel();
    }
    reader_task.abort();
    while tasks.join_next().await.is_some() {}
    result
}

pub async fn serve_stdio(engine: WorkflowEngine) -> Result<()> {
    serve(tokio::io::stdin(), tokio::io::stdout(), engine).await
}
