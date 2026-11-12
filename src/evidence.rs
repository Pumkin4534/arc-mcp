//! Evidence is produced by trusted execution, never by model-supplied exit codes.
use anyhow::{Context, Result, bail};
use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    future::Future,
    io::Read,
    path::{Path, PathBuf},
    pin::Pin,
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

pub const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_FILES: usize = 200_000;
const MAX_SOURCE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const EXCLUDED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    "coverage",
    ".next",
    ".cache",
    ".venv",
    "venv",
    "__pycache__",
    ".ssh",
    ".aws",
    ".gnupg",
];

#[derive(Clone, Debug)]
pub struct CheckRequest {
    pub project_root: PathBuf,
    pub command: String,
    pub timeout_secs: u64,
    pub max_output_bytes: usize,
    pub cancellation: CancellationToken,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckOutcome {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
    pub cancelled: bool,
    pub truncated: bool,
    /// Trusted runner implementation/protection provenance, not a model argument.
    pub runner: String,
}

pub trait CheckExecutor: Send + Sync {
    fn execute(
        &self,
        request: CheckRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CheckOutcome>> + Send + '_>>;
}

#[derive(Default)]
pub struct UnconfiguredExecutor;
impl CheckExecutor for UnconfiguredExecutor {
    fn execute(
        &self,
        _request: CheckRequest,
    ) -> Pin<Box<dyn Future<Output = Result<CheckOutcome>> + Send + '_>> {
        Box::pin(async {
            bail!(
                "guard_unconfigured: host must install the guarded CheckExecutor; no command executed"
            )
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceFingerprint {
    pub sha256: String,
    pub files: usize,
    pub bytes: u64,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CheckArtifact {
    pub id: String,
    pub workflow_id: String,
    pub revision: u64,
    pub task_ids: Vec<String>,
    pub command: String,
    pub started_at: String,
    pub finished_at: String,
    pub timeout_secs: u64,
    pub before: SourceFingerprint,
    pub after: Option<SourceFingerprint>,
    pub outcome: Option<CheckOutcome>,
    pub error: Option<String>,
    pub passed: bool,
}

fn excluded_name(name: &str) -> bool {
    #[cfg(windows)]
    let normalized = name.to_ascii_lowercase();
    #[cfg(windows)]
    let name = normalized.as_str();
    EXCLUDED_DIRS.contains(&name)
        || name == ".env"
        || name.starts_with(".env.")
        || matches!(
            name,
            "credentials" | "credentials.json" | "secrets.json" | "id_rsa" | "id_ed25519"
        )
        || [".pem", ".key", ".p12", ".pfx"]
            .iter()
            .any(|suffix| name.ends_with(suffix))
}

/// Source-only, non-Git-capable fingerprint. Exclusions are always disclosed.
/// This is a content freshness check, not an atomic filesystem snapshot.
pub fn fingerprint(root: &Path) -> Result<SourceFingerprint> {
    let start = Instant::now();
    let mut paths = Vec::new();
    let walker = WalkBuilder::new(root)
        .hidden(false)
        .require_git(false)
        .git_global(false)
        .follow_links(false)
        .filter_entry(|entry| {
            entry.depth() == 0 || !excluded_name(&entry.file_name().to_string_lossy())
        })
        .build();
    for entry in walker {
        let entry = entry.context("fingerprint_unknown: cannot enumerate source")?;
        if entry.depth() == 0 {
            continue;
        }
        if paths.len() >= MAX_FILES || start.elapsed() > Duration::from_secs(60) {
            bail!(
                "fingerprint_budget: source traversal exceeds 200000 entries or 60s; narrow the host project/ignore scope explicitly before checking"
            );
        }
        paths.push(entry.into_path());
    }
    paths.sort();
    let mut digest = Sha256::new();
    digest.update(b"arc-source-v1\0");
    let mut files = 0;
    let mut bytes = 0u64;
    let mut symlinks = 0;
    for path in paths {
        if start.elapsed() > Duration::from_secs(60) {
            bail!(
                "fingerprint_budget: source hashing exceeds 60s; no partial fingerprint accepted"
            );
        }
        let relative = path.strip_prefix(root)?;
        let metadata =
            fs::symlink_metadata(&path).context("fingerprint_unknown: source disappeared")?;
        if metadata.is_dir() {
            continue;
        }
        digest.update(relative.as_os_str().as_encoded_bytes());
        digest.update(b"\0");
        if metadata.file_type().is_symlink() {
            digest.update(b"symlink\0");
            digest.update(fs::read_link(&path)?.as_os_str().as_encoded_bytes());
            digest.update(b"\0");
            symlinks += 1;
            continue;
        }
        if !metadata.is_file() {
            bail!(
                "fingerprint_unknown: special file in source scope: {}",
                relative.display()
            );
        }
        bytes = bytes
            .checked_add(metadata.len())
            .context("fingerprint_budget: size overflow")?;
        if bytes > MAX_SOURCE_BYTES {
            bail!(
                "fingerprint_budget: eligible source exceeds 2GiB; explicitly ignore generated data or narrow the host project; no partial fingerprint accepted"
            );
        }
        let mut file = open_source_file(root, relative)?;
        let opened = file.metadata()?;
        if !opened.is_file() || opened.len() != metadata.len() {
            bail!(
                "fingerprint_unstable: source changed while opening {}",
                relative.display()
            );
        }
        digest.update(b"file\0");
        digest.update(opened.len().to_le_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            digest.update((opened.permissions().mode() & 0o111).to_le_bytes());
        }
        let mut read_bytes = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            read_bytes += count as u64;
            if read_bytes > opened.len() || start.elapsed() > Duration::from_secs(60) {
                bail!("fingerprint_unstable: source grew or hashing exceeded time budget");
            }
            digest.update(&buffer[..count]);
        }
        let after = file.metadata()?;
        if read_bytes != opened.len()
            || after.len() != opened.len()
            || after.modified()? != opened.modified()?
        {
            bail!(
                "fingerprint_unstable: source changed while hashing {}",
                relative.display()
            );
        }
        files += 1;
    }
    let mut warnings = vec![
        "Source scope respects .gitignore/.ignore and repository excludes (also without Git); global Git ignores are disabled. Generated directories (.git/.hg/.svn/node_modules/target/dist/build/coverage/.next/.cache/.venv/venv/__pycache__) and secret paths (.env*, .ssh/.aws/.gnupg, credentials, secrets.json, private-key extensions) are excluded. Checks cannot attest excluded content.".into(),
        "Fingerprint is a bounded source-content observation, not a filesystem snapshot; keep source quiescent during checks/review. Budgets: 200000 entries, 2GiB eligible files, 60 seconds; exceeding any budget fails closed.".into(),
    ];
    #[cfg(windows)]
    warnings.push("Windows fingerprints cover source paths/content, not ACLs or executable permissions. Reparse-point redirection is rejected before reading file content.".into());
    if symlinks > 0 {
        warnings.push(format!(
            "{symlinks} symlink target names hashed; referent content is outside attested scope."
        ));
    }
    Ok(SourceFingerprint {
        sha256: format!("{:x}", digest.finalize()),
        files,
        bytes,
        warnings,
    })
}

// Open every component relative to a directory descriptor without following links.
// This prevents a renamed ancestor/symlink from redirecting fingerprint reads.
#[cfg(unix)]
fn open_source_file(root: &Path, relative: &Path) -> Result<File> {
    use std::{
        ffi::CString,
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::ffi::OsStrExt,
        },
    };
    let mut directory = File::open(root)?;
    let components: Vec<_> = relative.components().collect();
    for (index, component) in components.iter().enumerate() {
        let std::path::Component::Normal(name) = component else {
            bail!("invalid source path");
        };
        let name = CString::new(name.as_bytes())?;
        let last = index + 1 == components.len();
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | if last { 0 } else { libc::O_DIRECTORY };
        // SAFETY: directory and NUL-terminated name remain live; returned fd is owned here.
        let fd = unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error())
                .context("fingerprint_unknown: safe source open failed");
        }
        // SAFETY: successful openat returned a new, uniquely owned descriptor.
        directory = unsafe { File::from_raw_fd(fd) };
    }
    Ok(directory)
}

#[cfg(windows)]
fn open_source_file(root: &Path, relative: &Path) -> Result<File> {
    use std::os::windows::{
        ffi::OsStringExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    };
    use windows_sys::Win32::{
        Globalization::{CSTR_EQUAL, CompareStringOrdinal},
        Storage::FileSystem::*,
    };
    let expected = root.join(relative);
    let file = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_SEQUENTIAL_SCAN)
        .open(&expected)?;
    if file.metadata()?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        bail!("fingerprint_unknown: reparse-point source file refused");
    }
    let mut buffer = vec![0u16; 32768];
    let length = unsafe {
        GetFinalPathNameByHandleW(
            file.as_raw_handle(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            0,
        )
    };
    if length == 0 || length as usize >= buffer.len() {
        bail!("fingerprint_unknown: cannot resolve opened Windows source handle");
    }
    let actual = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length as usize]));
    // Compare the opened HANDLE's path, not a second pathname lookup. This rejects
    // ancestor junction/reparse redirection before any file content is read.
    fn normalized(path: &Path) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        let value: Vec<u16> = path.as_os_str().encode_wide().collect();
        let unc: Vec<u16> = "\\\\?\\UNC\\".encode_utf16().collect();
        let prefix: Vec<u16> = "\\\\?\\".encode_utf16().collect();
        if value.starts_with(&unc) {
            return "\\\\"
                .encode_utf16()
                .chain(value[unc.len()..].iter().copied())
                .collect();
        }
        if value.starts_with(&prefix) {
            return value[prefix.len()..].to_vec();
        }
        value
    }
    let expected = normalized(&expected);
    let actual = normalized(&actual);
    if unsafe {
        CompareStringOrdinal(
            expected.as_ptr(),
            expected.len() as i32,
            actual.as_ptr(),
            actual.len() as i32,
            1,
        )
    } != CSTR_EQUAL
    {
        bail!("fingerprint_unknown: Windows ancestor reparse/rename redirected source");
    }
    Ok(file)
}

#[cfg(not(any(unix, windows)))]
fn open_source_file(_root: &Path, _relative: &Path) -> Result<File> {
    bail!("fingerprint_unsupported: no no-follow source reader implemented for this platform")
}

pub(crate) fn bound_text(value: &mut String, max: usize) -> bool {
    if value.len() <= max {
        return false;
    }
    let mut end = max;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    true
}

pub(crate) fn bound_output(outcome: &mut CheckOutcome) {
    outcome.truncated |= bound_text(&mut outcome.stdout, MAX_OUTPUT_BYTES / 2);
    outcome.truncated |= bound_text(&mut outcome.stderr, MAX_OUTPUT_BYTES / 2);
    // Provenance is a bounded label; raw child output belongs above.
    outcome.truncated |= bound_text(&mut outcome.runner, 1024);
}
