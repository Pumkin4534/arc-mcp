//! Explicit native execution for Windows/Ubuntu. This is NOT a filesystem or
//! network sandbox. Evidence records the runner's actual protection boundary.
use crate::evidence::{CheckExecutor, CheckOutcome, CheckRequest};
use anyhow::{Context, Result, bail, ensure};
use std::{future::Future, path::PathBuf, pin::Pin, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
};

pub struct ProcessCheckExecutor {
    helper: PathBuf,
}
impl ProcessCheckExecutor {
    pub fn new(helper: PathBuf) -> Self {
        Self { helper }
    }

    async fn run(&self, request: CheckRequest) -> Result<CheckOutcome> {
        ensure!(
            !request.cancellation.is_cancelled(),
            "cancelled before command launch"
        );
        let scratch = tempfile::tempdir()?;
        #[cfg(windows)]
        let mut command = {
            let mut cmd = Command::new(&self.helper);
            cmd.arg("native-check-child");
            cmd
        };
        #[cfg(unix)]
        let mut command = {
            let _ = &self.helper;
            let mut cmd = Command::new("/bin/sh");
            cmd.args(["-c", &request.command]);
            cmd
        };
        #[cfg(windows)]
        command.current_dir(dunce::simplified(&request.project_root));
        #[cfg(not(windows))]
        command.current_dir(&request.project_root);
        command
            .env_clear()
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for name in [
            "PATH",
            "HOME",
            "USERPROFILE",
            "HOMEDRIVE",
            "HOMEPATH",
            "SYSTEMROOT",
            "SystemDrive",
            "ProgramFiles",
            "ProgramFiles(x86)",
            "ProgramW6432",
            "WINDIR",
            "COMSPEC",
            "PATHEXT",
            "APPDATA",
            "LOCALAPPDATA",
            "CARGO_HOME",
            "RUSTUP_HOME",
            "RUSTUP_TOOLCHAIN",
            "JAVA_HOME",
            "VIRTUAL_ENV",
        ] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        command
            .env("TMPDIR", scratch.path())
            .env("TMP", scratch.path())
            .env("TEMP", scratch.path())
            .env("NO_COLOR", "1")
            .env("GIT_OPTIONAL_LOCKS", "0");
        #[cfg(unix)]
        {
            command.stdin(Stdio::null()).process_group(0);
        }
        #[cfg(windows)]
        command.stdin(Stdio::piped());
        let mut child = command
            .spawn()
            .context("native_runner: cannot start check")?;
        let mut lifetime = ProcessLifetime::attach(&child)?;
        #[cfg(windows)]
        let input = {
            use tokio::io::AsyncWriteExt;
            let mut stdin = child.stdin.take().context("missing helper input")?;
            let bytes = serde_json::to_vec(&serde_json::json!({"command":request.command}))?;
            tokio::spawn(async move {
                stdin.write_all(&bytes).await?;
                stdin.shutdown().await
            })
        };
        let cap = request.max_output_bytes.clamp(1024, 65536) / 2;
        let mut stdout = tokio::spawn(capture(child.stdout.take().context("missing stdout")?, cap));
        let mut stderr = tokio::spawn(capture(child.stderr.take().context("missing stderr")?, cap));
        let mut timed_out = false;
        let mut cancelled = false;
        let status = tokio::select! {
            status = child.wait() => status?,
            _ = request.cancellation.cancelled() => { cancelled = true; lifetime.terminate()?; child.wait().await? }
            _ = tokio::time::sleep(Duration::from_secs(request.timeout_secs)) => { timed_out = true; lifetime.terminate()?; child.wait().await? }
        };
        lifetime.terminate()?;
        #[cfg(windows)]
        {
            let sent = input.await?;
            if !cancelled && !timed_out {
                sent.context("native_runner: helper input failed")?;
            }
        }
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
                    "check_cleanup_unknown: output remained open after owned-process cleanup; inspect delegated/detached processes before retrying"
                )
            }
        };
        Ok(CheckOutcome {
            exit_code: status.code(),
            stdout: out,
            stderr: err,
            timed_out,
            cancelled,
            truncated: out_truncated || err_truncated,
            runner: format!(
                "{} native {} command; NO filesystem/network sandbox; cleanup limited to {} (not external services or delegated processes)",
                std::env::consts::OS,
                if cfg!(windows) {
                    "PowerShell"
                } else {
                    "POSIX shell"
                },
                if cfg!(windows) {
                    "the owned Windows Job Object"
                } else {
                    "the owned process group; detached descendants are not guaranteed"
                }
            ),
        })
    }
}

impl CheckExecutor for ProcessCheckExecutor {
    fn execute(
        &self,
        request: CheckRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CheckOutcome>> + Send + '_>> {
        Box::pin(self.run(request))
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
        let kept = count.min(cap.saturating_sub(bytes.len()));
        bytes.extend_from_slice(&buffer[..kept]);
        truncated |= kept < count;
    }
    Ok((String::from_utf8_lossy(&bytes).into_owned(), truncated))
}

#[cfg(unix)]
struct ProcessLifetime(Option<u32>);
#[cfg(unix)]
impl ProcessLifetime {
    fn attach(child: &tokio::process::Child) -> Result<Self> {
        Ok(Self(Some(child.id().context("missing process ID")?)))
    }
    fn terminate(&mut self) -> Result<()> {
        if let Some(pid) = self.0.take() {
            // SAFETY: this child was started in a new, owned process group.
            if unsafe { libc::kill(-(pid as i32), libc::SIGKILL) } != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error).context("native_runner: process-group cleanup failed");
                }
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
struct ProcessLifetime {
    handle: std::os::windows::io::OwnedHandle,
    stopped: bool,
}
#[cfg(windows)]
impl ProcessLifetime {
    fn attach(child: &tokio::process::Child) -> Result<Self> {
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::System::JobObjects::*;
        // The trusted helper waits for stdin. No user command is sent until it
        // has joined this kill-on-close job, eliminating the spawn/assign race.
        let raw = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        ensure!(
            !raw.is_null(),
            "native_runner: cannot create Windows job: {}",
            std::io::Error::last_os_error()
        );
        let handle = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        let configured = unsafe {
            SetInformationJobObject(
                handle.as_raw_handle(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                std::mem::size_of_val(&limits) as u32,
            )
        };
        ensure!(
            configured != 0,
            "native_runner: cannot configure Windows job: {}",
            std::io::Error::last_os_error()
        );
        let process = child.raw_handle().context("missing Windows child handle")?;
        ensure!(
            unsafe { AssignProcessToJobObject(handle.as_raw_handle(), process) } != 0,
            "native_runner: cannot assign Windows job: {}",
            std::io::Error::last_os_error()
        );
        Ok(Self {
            handle,
            stopped: false,
        })
    }
    fn terminate(&mut self) -> Result<()> {
        use std::os::windows::io::AsRawHandle;
        if !self.stopped {
            ensure!(
                unsafe {
                    windows_sys::Win32::System::JobObjects::TerminateJobObject(
                        self.handle.as_raw_handle(),
                        1,
                    )
                } != 0,
                "native_runner: Windows job cleanup failed: {}",
                std::io::Error::last_os_error()
            );
            self.stopped = true;
        }
        Ok(())
    }
}
impl Drop for ProcessLifetime {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

#[cfg(windows)]
pub async fn child_main() -> Result<()> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Input {
        command: String,
    }
    let mut bytes = Vec::new();
    tokio::io::stdin()
        .take(128 * 1024 + 1)
        .read_to_end(&mut bytes)
        .await?;
    ensure!(bytes.len() <= 128 * 1024, "native helper input too large");
    let input: Input = serde_json::from_slice(&bytes)?;
    ensure!(
        !input.command.trim().is_empty() && input.command.len() <= 16000,
        "invalid native helper command"
    );
    let shell = PathBuf::from(std::env::var_os("SYSTEMROOT").context("SYSTEMROOT missing")?)
        .join("System32/WindowsPowerShell/v1.0/powershell.exe");
    // Do not inherit PowerShell 7's module path into Windows PowerShell 5.1.
    // Its built-in file/process cmdlets must be resolvable after env_clear.
    let mut modules = vec![
        shell
            .parent()
            .context("PowerShell directory missing")?
            .join("Modules"),
    ];
    if let Some(path) = std::env::var_os("ProgramFiles") {
        modules.push(PathBuf::from(path).join("WindowsPowerShell/Modules"));
    }
    if let Some(path) = std::env::var_os("USERPROFILE") {
        modules.push(PathBuf::from(path).join("Documents/WindowsPowerShell/Modules"));
    }
    let script = format!(
        "[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false); $OutputEncoding = [Console]::OutputEncoding; {}",
        input.command
    );
    let status = Command::new(shell)
        .env("PSModulePath", std::env::join_paths(modules)?)
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &script,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .await?;
    std::process::exit(status.code().unwrap_or(1));
}
