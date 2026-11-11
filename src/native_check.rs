//! Scoped macOS protection for MCP-owned checks. Native OpenCode tools are not
//! intercepted. Cleanup owns a process group, not arbitrary detached processes.
use crate::evidence::{CheckExecutor, CheckOutcome, CheckRequest};
use anyhow::{Context, Result, bail, ensure};
use std::{
    future::Future,
    path::{Path, PathBuf},
    pin::Pin,
    process::Stdio,
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

pub struct NativeCheckExecutor {
    protected: Vec<PathBuf>,
    private: Vec<PathBuf>,
}

impl NativeCheckExecutor {
    pub fn new(protected: Vec<PathBuf>, private: Vec<PathBuf>) -> Result<Self> {
        ensure!(
            cfg!(target_os = "macos"),
            "guard_unavailable: this release requires macOS Seatbelt; no unguarded fallback"
        );
        let protected = protected
            .into_iter()
            .map(std::fs::canonicalize)
            .collect::<std::io::Result<Vec<_>>>()?;
        let private = private
            .into_iter()
            .map(std::fs::canonicalize)
            .collect::<std::io::Result<Vec<_>>>()?;
        // Path-based profiles cannot protect an unknown existing hardlink alias.
        // Refuse those identities rather than claiming containment of the alias.
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let start = std::time::Instant::now();
            let mut count = 0;
            for root in protected.iter().chain(&private) {
                for entry in ignore::WalkBuilder::new(root)
                    .standard_filters(false)
                    .follow_links(false)
                    .build()
                {
                    let entry = entry.context("guard_inventory: cannot inspect protected path")?;
                    count += 1;
                    ensure!(
                        count <= 200_000 && start.elapsed() < Duration::from_secs(20),
                        "guard_inventory: protected identity scan exceeded budget"
                    );
                    let metadata = std::fs::symlink_metadata(entry.path())?;
                    ensure!(
                        !metadata.is_file() || metadata.nlink() == 1,
                        "guard_inventory: protected file has an unaccounted hardlink: {}",
                        entry.path().display()
                    );
                }
            }
        }
        Ok(Self { protected, private })
    }

    fn profile(&self, root: &Path, scratch: &Path) -> Result<String> {
        fn quote(path: &Path) -> Result<String> {
            let text = path.to_str().context("guard_path: non-UTF8 path")?;
            ensure!(
                !text.chars().any(char::is_control),
                "guard_path: control character in path"
            );
            Ok(format!(
                "\"{}\"",
                text.replace('\\', "\\\\").replace('"', "\\\"")
            ))
        }
        let mut profile = format!(
            "(version 1)(allow default)(deny file-write*)(allow file-write* (literal \"/dev/null\") (subpath {}) (subpath {}))(deny network*)(deny mach-lookup)(deny process-info*)(allow process-info* (target self))",
            quote(root)?,
            quote(scratch)?
        );
        for path in self.protected.iter().chain(&self.private) {
            profile.push_str(&format!(
                "(deny file-write* (literal {0}) (subpath {0}))",
                quote(path)?
            ));
            // Deny replacement by renaming an enclosing directory, without
            // denying writes to unrelated siblings (e.g. browser-output).
            for ancestor in path.ancestors().skip(1).filter(|p| p.parent().is_some()) {
                profile.push_str(&format!(
                    "(deny file-write-unlink (literal {}))",
                    quote(ancestor)?
                ));
            }
        }
        for path in &self.private {
            profile.push_str(&format!(
                "(deny file-read* (literal {0}) (subpath {0}))",
                quote(path)?
            ));
        }
        Ok(profile)
    }

    async fn run(&self, request: CheckRequest) -> Result<CheckOutcome> {
        ensure!(
            !request.cancellation.is_cancelled(),
            "cancelled before command launch"
        );
        let scratch = tempfile::tempdir()?;
        let scratch_path = std::fs::canonicalize(scratch.path())?;
        let root = std::fs::canonicalize(&request.project_root)?;
        let profile = self.profile(&root, &scratch_path)?;
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command
            .args(["-p", &profile, "/bin/sh", "-c", &request.command])
            .current_dir(&root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env_clear()
            .env(
                "PATH",
                std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin:/usr/sbin:/sbin".into()),
            )
            .env("HOME", dirs::home_dir().context("missing home directory")?)
            .env("TMPDIR", &scratch_path)
            .env("TMP", &scratch_path)
            .env("TEMP", &scratch_path)
            .env("LANG", "en_US.UTF-8")
            .env("NO_COLOR", "1")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let mut child = command
            .spawn()
            .context("guard_unavailable: sandbox-exec failed; no unguarded retry")?;
        let pid = child.id().context("missing check process ID")?;
        let cap = request.max_output_bytes.clamp(1024, 65536) / 2;
        let stdout = tokio::spawn(capture(child.stdout.take().context("missing stdout")?, cap));
        let stderr = tokio::spawn(capture(child.stderr.take().context("missing stderr")?, cap));
        let mut timed_out = false;
        let mut cancelled = false;
        let status = tokio::select! {
            status = child.wait() => status?,
            _ = request.cancellation.cancelled() => {
                cancelled = true;
                kill_group(pid);
                child.wait().await?
            }
            _ = tokio::time::sleep(Duration::from_secs(request.timeout_secs)) => {
                timed_out = true;
                kill_group(pid);
                child.wait().await?
            }
        };
        kill_group(pid); // Also clean up ordinary background descendants on success.
        let mut stdout = stdout;
        let mut stderr = stderr;
        let output = tokio::time::timeout(Duration::from_secs(2), async {
            Ok::<_, anyhow::Error>(((&mut stdout).await??, (&mut stderr).await??))
        })
        .await;
        let ((out, out_truncated), (err, err_truncated)) = match output {
            Ok(result) => result?,
            Err(_) => {
                stdout.abort();
                stderr.abort();
                bail!(
                    "check_cleanup_unknown: output pipes remained open after process-group cleanup; inspect detached processes before retrying"
                )
            }
        };
        Ok(CheckOutcome { exit_code: status.code(), stdout: out, stderr: err, timed_out, cancelled,
            truncated: out_truncated || err_truncated,
            runner: "macOS Seatbelt: project/scratch writes, protected-path/private-state denial, no network/Mach delegation; process-group cleanup only (detached processes not guaranteed)".into() })
    }
}

fn kill_group(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
}

async fn capture(mut stream: impl AsyncRead + Unpin, cap: usize) -> Result<(String, bool)> {
    let mut bytes = Vec::new();
    let mut truncated = false;
    let mut buffer = [0; 8192];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let retained = count.min(cap.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..retained]);
        truncated |= retained < count;
    }
    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}

impl CheckExecutor for NativeCheckExecutor {
    fn execute(
        &self,
        request: CheckRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CheckOutcome>> + Send + '_>> {
        Box::pin(self.run(request))
    }
}
