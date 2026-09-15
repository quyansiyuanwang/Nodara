// Prevents an additional console window on Windows in release builds. Debug
// builds intentionally keep the console for runtime/tracing output.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Desktop shell for the Studio.
//!
//! The shell loads the same web bundle as the browser build. It also owns the
//! lifecycle of a sibling runtime process so a packaged Studio is a one-click
//! experience: if port 8710 is already served, that runtime is reused; if not,
//! `nodara-runtime.exe` is started from the package and stopped with Studio.

use std::net::{SocketAddr, TcpStream};
#[cfg(windows)]
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use tauri::RunEvent;

const RUNTIME_ADDRESS: &str = "127.0.0.1:8710";
const RUNTIME_START_TIMEOUT: Duration = Duration::from_secs(8);

fn main() {
    let mut runtime = RuntimeProcess::start();

    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![run_agent_turn])
        .build(tauri::generate_context!())
        .expect("error while building the Studio shell");

    app.run(move |_app, event| {
        if matches!(event, RunEvent::Exit) {
            runtime.stop();
        }
    });
}

/// Run one structured Agent turn through the sibling agent binary.
#[tauri::command]
fn run_agent_turn(request: serde_json::Value) -> Result<serde_json::Value, String> {
    let runtime = request
        .get("runtime_url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("http://127.0.0.1:8710");
    let executable = find_agent_binary().ok_or_else(|| {
        "nodara-agent.exe was not found; set NODARA_AGENT_BIN or install the desktop bundle"
            .to_string()
    })?;
    let mut child = Command::new(&executable)
        .arg("--runtime")
        .arg(runtime)
        .arg("studio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", executable.display()))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let payload = serde_json::to_vec(&request)
            .map_err(|error| format!("could not encode the Agent request: {error}"))?;
        stdin
            .write_all(&payload)
            .map_err(|error| format!("could not send the Agent request: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("could not wait for the Agent: {error}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("Agent exited with {}", output.status)
        } else {
            stderr
        });
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("Agent returned invalid JSON: {error}"))
}

fn find_agent_binary() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("NODARA_AGENT_BIN") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    let current = std::env::current_exe().ok()?;
    for ancestor in current.ancestors() {
        for profile in ["debug", "release"] {
            let candidate = ancestor
                .join("Nodara-Agent")
                .join("target")
                .join(profile)
                .join("nodara-agent.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    let sibling = current.parent()?.join("nodara-agent.exe");
    sibling.is_file().then_some(sibling)
}

/// A runtime started by this Studio instance.
///
/// `child` is `None` when an existing runtime already owns port 8710. That
/// distinction is important: a manually started runtime must survive Studio
/// closing.
struct RuntimeProcess {
    child: Option<Child>,
    #[cfg(windows)]
    _job: Option<OwnedHandle>,
}

impl RuntimeProcess {
    fn start() -> Self {
        if runtime_is_reachable() {
            eprintln!("Nodara Studio: reusing the runtime already listening on {RUNTIME_ADDRESS}");
            return Self::without_child();
        }

        let Some(executable) = find_runtime_binary() else {
            eprintln!(
                "Nodara Studio: nodara-runtime.exe was not found; start it manually or set NODARA_RUNTIME_BIN"
            );
            return Self::without_child();
        };

        match spawn_runtime(&executable) {
            Ok(mut child) => {
                if wait_until_reachable(&mut child) {
                    eprintln!(
                        "Nodara Studio: started {} on {RUNTIME_ADDRESS}",
                        executable.display()
                    );
                } else {
                    eprintln!(
                        "Nodara Studio: runtime did not become ready within {} seconds",
                        RUNTIME_START_TIMEOUT.as_secs()
                    );
                }
                Self::with_child(child)
            }
            Err(error) => {
                eprintln!(
                    "Nodara Studio: could not start {}: {error}",
                    executable.display()
                );
                Self::without_child()
            }
        }
    }

    fn without_child() -> Self {
        Self {
            child: None,
            #[cfg(windows)]
            _job: None,
        }
    }

    fn with_child(child: Child) -> Self {
        #[cfg(windows)]
        let job = match assign_to_job(&child) {
            Ok(job) => Some(job),
            Err(error) => {
                eprintln!("Nodara Studio: could not bind the runtime to a job object: {error}");
                None
            }
        };

        Self {
            child: Some(child),
            #[cfg(windows)]
            _job: job,
        }
    }

    fn stop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let _ = child.kill();
        let _ = child.wait();
    }
}

impl Drop for RuntimeProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

fn runtime_is_reachable() -> bool {
    let Ok(address) = RUNTIME_ADDRESS.parse::<SocketAddr>() else {
        return false;
    };
    TcpStream::connect_timeout(&address, Duration::from_millis(150)).is_ok()
}

fn wait_until_reachable(child: &mut Child) -> bool {
    let deadline = Instant::now() + RUNTIME_START_TIMEOUT;
    while Instant::now() < deadline {
        if runtime_is_reachable() {
            return true;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                eprintln!("Nodara Studio: runtime exited during startup with {status}");
                return false;
            }
            Ok(None) => thread::sleep(Duration::from_millis(75)),
            Err(error) => {
                eprintln!("Nodara Studio: could not monitor the runtime process: {error}");
                return false;
            }
        }
    }
    false
}

fn spawn_runtime(executable: &Path) -> std::io::Result<Child> {
    let mut command = Command::new(executable);
    if let Some(directory) = executable.parent() {
        command.current_dir(directory);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Keep the debug console attached to Studio for logs, but do not make
        // the child open a second console window.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command.spawn()
}

fn find_runtime_binary() -> Option<PathBuf> {
    let filename = format!("nodara-runtime{}", std::env::consts::EXE_SUFFIX);
    let mut candidates = Vec::new();

    if let Some(configured) = std::env::var_os("NODARA_RUNTIME_BIN") {
        candidates.push(PathBuf::from(configured));
    }

    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(directory) = current_exe.parent() {
            candidates.push(directory.join(&filename));
            add_workspace_candidates(&mut candidates, directory, &filename);
        }
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join(&filename));
        add_workspace_candidates(&mut candidates, &current_dir, &filename);
    }

    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn add_workspace_candidates(candidates: &mut Vec<PathBuf>, start: &Path, filename: &str) {
    let preferred = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let fallback = if cfg!(debug_assertions) {
        "release"
    } else {
        "debug"
    };

    for ancestor in start.ancestors().take(7) {
        for profile in [preferred, fallback] {
            candidates.push(
                ancestor
                    .join("Nodara-Core")
                    .join("target")
                    .join(profile)
                    .join(filename),
            );
        }
    }
}
/// Ensure the child is killed if Studio crashes or is force-terminated.
/// Closing the last handle to a job with this limit terminates its processes.
#[cfg(windows)]
fn assign_to_job(child: &Child) -> std::io::Result<OwnedHandle> {
    use std::ffi::c_void;
    use std::mem::size_of;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err(std::io::Error::last_os_error());
    }

    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let configured = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const c_void,
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if configured == 0 {
        let error = std::io::Error::last_os_error();
        unsafe {
            CloseHandle(job);
        }
        return Err(error);
    }

    let assigned = unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) };
    if assigned == 0 {
        let error = std::io::Error::last_os_error();
        unsafe {
            CloseHandle(job);
        }
        return Err(error);
    }

    Ok(unsafe { OwnedHandle::from_raw_handle(job) })
}
