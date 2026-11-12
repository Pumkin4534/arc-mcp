use anyhow::{Context, Result, ensure};
#[cfg(target_os = "macos")]
use arc_mcp::native_check::NativeCheckExecutor;
use arc_mcp::{process_check::ProcessCheckExecutor, workflow::WorkflowEngine};
use clap::{Parser, Subcommand, ValueEnum};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{path::PathBuf, sync::Arc};
use tokio::io::AsyncReadExt;

#[derive(Parser)]
#[command(
    version,
    about = "Native OpenCode workflow/evidence MCP (advisory host records)"
)]
struct Cli {
    #[arg(long, global = true)]
    root: Option<PathBuf>,
    #[arg(long, global = true)]
    data_home: Option<PathBuf>,
    /// Optional development source protection, selected at runtime by the host.
    #[arg(long, global = true)]
    source_root: Option<PathBuf>,
    /// Platform default: Seatbelt on macOS, explicit native execution on Windows/Linux.
    #[arg(long, global = true, value_enum, default_value = "platform")]
    runner: Runner,
    #[command(subcommand)]
    command: Action,
}

#[derive(Clone, Copy, ValueEnum)]
enum Runner {
    Platform,
    Seatbelt,
    Native,
}

#[derive(Subcommand)]
enum Action {
    /// Serve newline MCP on stdin/stdout, using OpenCode's working directory.
    Serve,
    /// Inspect the current native-advisory workflow without running any command.
    Status,
    /// Local plugin IPC. NOT an authenticated or protected attestation channel.
    NativeHost,
    #[cfg(windows)]
    #[command(hide = true)]
    NativeCheckChild,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HostRequest {
    version: u8,
    id: String,
    operation: String,
    payload: Value,
}

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("arc-mcp: {error:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    #[cfg(windows)]
    if matches!(&cli.command, Action::NativeCheckChild) {
        return arc_mcp::process_check::child_main().await;
    }
    let root = match cli.root {
        Some(root) => root,
        None => std::env::current_dir()?,
    };
    let home = match cli
        .data_home
        .or_else(|| std::env::var_os("ARC_DATA_HOME").map(PathBuf::from))
    {
        Some(home) => home,
        None => dirs::data_local_dir()
            .context("missing data directory")?
            .join("arc-mcp"),
    };
    let engine = WorkflowEngine::new(&root, &home)?.with_native_mode();
    match cli.command {
        Action::Serve => {
            let native = matches!(cli.runner, Runner::Native)
                || (matches!(cli.runner, Runner::Platform) && !cfg!(target_os = "macos"));
            if native {
                ensure!(
                    cli.source_root.is_none(),
                    "--source-root protection requires Seatbelt; native checks are not sandboxed"
                );
                ensure!(
                    !cfg!(target_os = "macos"),
                    "macOS MCP checks require Seatbelt; no native fallback"
                );
                let executor = ProcessCheckExecutor::new(std::env::current_exe()?);
                return arc_mcp::protocol::serve_stdio(engine.with_executor(Arc::new(executor)))
                    .await;
            }
            #[cfg(not(target_os = "macos"))]
            anyhow::bail!(
                "Seatbelt is only available on macOS; select --runner native for non-sandboxed Windows/Linux checks"
            );
            #[cfg(target_os = "macos")]
            {
                let mut protected = vec![std::env::current_exe()?];
                // No builder's home/source path is embedded in distributed binaries.
                if let Some(source) = cli.source_root {
                    for relative in [
                        "src",
                        "integration",
                        "scripts",
                        "tests",
                        "Cargo.toml",
                        "Cargo.lock",
                    ] {
                        let path = source.join(relative);
                        if path.exists() {
                            protected.push(path);
                        }
                    }
                }
                if let Some(user) = dirs::home_dir() {
                    let opencode = user.join(".opencode/bin/opencode");
                    if opencode.exists() {
                        protected.push(opencode);
                    }
                    let config = std::env::var_os("XDG_CONFIG_HOME")
                        .map(PathBuf::from)
                        .unwrap_or_else(|| user.join(".config"))
                        .join("opencode");
                    for relative in [
                        "opencode.jsonc",
                        "opencode.json",
                        "arc-mcp",
                        "plugins/arc-opencode.js",
                        "agents/arc-reviewer.md",
                        "skills/arc-workflow",
                        "commands/arc.md",
                        "commands/arc-status.md",
                        "commands/arc-continue.md",
                    ] {
                        let path = config.join(relative);
                        if path.exists() {
                            protected.push(path);
                        }
                    }
                }
                let executor = NativeCheckExecutor::new(protected, vec![home])?;
                arc_mcp::protocol::serve_stdio(engine.with_executor(Arc::new(executor))).await
            }
        }
        Action::Status => {
            println!("{}", engine.assurance(engine.status()?));
            Ok(())
        }
        Action::NativeHost => {
            // This is intentionally local, unauthenticated native-mode IPC.
            // It cannot open protected-mode state; no secret or security claim.
            let mut bytes = Vec::new();
            tokio::io::stdin()
                .take(256 * 1024 + 1)
                .read_to_end(&mut bytes)
                .await?;
            ensure!(bytes.len() <= 256 * 1024, "native request exceeds 256KiB");
            let request: HostRequest = serde_json::from_slice(&bytes)?;
            ensure!(
                request.version == 1 && !request.id.is_empty() && request.id.len() <= 200,
                "invalid native envelope"
            );
            let result = match request.operation.as_str() {
                "status" | "prepare_review" => {
                    ensure!(
                        request.payload == json!({}),
                        "operation requires empty payload"
                    );
                    let tool = if request.operation == "status" {
                        "arc_status"
                    } else {
                        "arc_prepare_review"
                    };
                    engine
                        .call_tool(tool, json!({}), tokio_util::sync::CancellationToken::new())
                        .await
                }
                "bind" => engine.host_bind(serde_json::from_value(request.payload)?),
                "accept_review" => {
                    engine.host_accept_review(serde_json::from_value(request.payload)?)
                }
                "recover_check" => {
                    // Operator-only convention, not an authentication boundary:
                    // caller must inspect/stop the old process group first.
                    engine.host_recover_check(serde_json::from_value(request.payload)?)
                }
                _ => anyhow::bail!("unknown native operation"),
            };
            let response = match result {
                Ok(value) => {
                    json!({"version":1,"id":request.id,"ok":true,"result":engine.assurance(value)})
                }
                Err(error) => {
                    json!({"version":1,"id":request.id,"ok":false,"error":format!("{error:#}")})
                }
            };
            println!("{response}");
            Ok(())
        }
        #[cfg(windows)]
        Action::NativeCheckChild => unreachable!("helper handled before workflow construction"),
    }
}
