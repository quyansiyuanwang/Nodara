//! External process execution.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use nodara_core::{
    ExecutionContext, NodeError, NodeExecutor, NodeInput, NodeOutput, NodeResult, RunControl,
};
use nodara_schema::{NodeDescriptor, PortDescriptor, PortKind, ValueType};

const POLL_INTERVAL: Duration = Duration::from_millis(20);

fn port(name: &str, display: &str, kind: PortKind, value_type: ValueType) -> PortDescriptor {
    PortDescriptor::new(name, display, kind, value_type)
}

#[derive(Debug, Clone, serde::Deserialize)]
struct CommandConfig {
    program: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    shell: bool,
    #[serde(default)]
    stdin: Option<String>,
    #[serde(default = "default_true")]
    check_exit_code: bool,
    #[serde(default = "default_true")]
    wait: bool,
}

fn default_true() -> bool {
    true
}

impl CommandConfig {
    fn from_input(input: &NodeInput) -> NodeResult<Self> {
        let mut config: Self =
            serde_json::from_value(input.resolved_config.clone()).map_err(|error| {
                NodeError::InvalidConfig(format!("invalid command config: {error}"))
            })?;
        if config.program.trim().is_empty() {
            return Err(NodeError::InvalidConfig(
                "`program` must not be empty".to_string(),
            ));
        }
        config.cwd = config.cwd.filter(|directory| !directory.trim().is_empty());
        Ok(config)
    }
}

#[derive(Debug, Clone)]
struct ProcessResult {
    pid: u32,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
}

/// `system.Command`
#[derive(Debug, Default)]
pub struct CommandExecutor;

impl NodeExecutor for CommandExecutor {
    fn descriptor(&self) -> NodeDescriptor {
        NodeDescriptor {
            inputs: vec![port("in", "In", PortKind::Input, ValueType::Any)],
            outputs: vec![
                port("out", "Stdout", PortKind::Output, ValueType::String),
                port("stderr", "Stderr", PortKind::Output, ValueType::String),
                port(
                    "exit_code",
                    "Exit code",
                    PortKind::Output,
                    ValueType::Number,
                ),
                port("success", "Success", PortKind::Output, ValueType::Boolean),
                port("pid", "Process id", PortKind::Output, ValueType::Number),
            ],
            config_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "program": {
                        "type": "string",
                        "title": "Program or command",
                        "description": "Executable to launch. With `shell` enabled this can also be a shell command or Windows built-in such as `dir`.",
                        "examples": ["python", "powershell", "echo done"]
                    },
                    "args": {
                        "type": "array",
                        "title": "Arguments",
                        "description": "Arguments passed to the program. Each item is quoted by the operating system.",
                        "items": { "type": "string" },
                        "default": []
                    },
                    "cwd": {
                        "type": "string",
                        "title": "Working directory",
                        "description": "Directory in which to launch the process. Blank uses the plugin process directory."
                    },
                    "env": {
                        "type": "object",
                        "title": "Environment variables",
                        "description": "Additional environment variables merged into the inherited environment.",
                        "additionalProperties": { "type": "string" },
                        "default": {}
                    },
                    "shell": {
                        "type": "boolean",
                        "title": "Run through shell",
                        "description": "Use cmd.exe on Windows or sh on other systems. Disable to launch the program directly.",
                        "default": true
                    },
                    "stdin": {
                        "type": "string",
                        "title": "Standard input",
                        "description": "Optional text written to the process standard input. Supports `{{variable}}` interpolation."
                    },
                    "check_exit_code": {
                        "type": "boolean",
                        "title": "Fail on non-zero exit",
                        "description": "Fail the node when the process exits non-zero. Disable to inspect exit_code, stdout and stderr in downstream nodes.",
                        "default": true
                    },
                    "wait": {
                        "type": "boolean",
                        "title": "Wait for completion",
                        "description": "Wait for the process and capture output. Disable to launch it in the background and return its pid.",
                        "default": true
                    }
                },
                "required": ["program"],
                "additionalProperties": false
            }),
            dangerous: true,
            permissions: vec!["process.execute".to_string()],
            allows_additional_config: false,
            ..NodeDescriptor::new("system.Command", "Command", "System")
                .with_description("Launches an external program or shell command")
        }
    }

    fn execute(&self, input: NodeInput, context: &mut ExecutionContext) -> NodeResult<NodeOutput> {
        context.check_cancelled()?;
        let config = CommandConfig::from_input(&input)?;
        let timeout = input.timeout_ms.map(Duration::from_millis);
        run_command(&config, context.control(), timeout)
    }
}

fn run_command(
    config: &CommandConfig,
    control: &RunControl,
    timeout: Option<Duration>,
) -> NodeResult<NodeOutput> {
    let result = run_process(config, control, timeout)?;
    let success = result.exit_code.map_or(true, |code| code == 0);

    if config.check_exit_code && result.exit_code.is_some_and(|code| code != 0) {
        let stderr = result.stderr.trim();
        let stdout = result.stdout.trim();
        let detail = if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            "no diagnostic output"
        };
        return Err(NodeError::Execution(format!(
            "command exited with code {}: {detail}",
            result.exit_code.unwrap_or_default()
        )));
    }

    Ok(NodeOutput::new()
        .with_output("out", serde_json::json!(result.stdout))
        .with_output("stderr", serde_json::json!(result.stderr))
        .with_output("exit_code", serde_json::json!(result.exit_code))
        .with_output("success", serde_json::json!(success))
        .with_output("pid", serde_json::json!(result.pid)))
}

fn run_process(
    config: &CommandConfig,
    control: &RunControl,
    timeout: Option<Duration>,
) -> NodeResult<ProcessResult> {
    let mut command = build_command(config);
    command
        .stdin(if config.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(if config.wait {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stderr(if config.wait {
            Stdio::piped()
        } else {
            Stdio::null()
        });

    let mut child = command
        .spawn()
        .map_err(|error| NodeError::Execution(format!("could not start process: {error}")))?;
    let pid = child.id();

    if let Some(text) = &config.stdin {
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes()).map_err(|error| {
                let _ = child.kill();
                let _ = child.wait();
                NodeError::Io(error.to_string())
            })?;
        }
    }

    if !config.wait {
        return Ok(ProcessResult {
            pid,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        });
    }

    let stdout = child.stdout.take().map(|stream| {
        std::thread::spawn(move || {
            let mut stream = stream;
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).map(|_| bytes)
        })
    });
    let stderr = child.stderr.take().map(|stream| {
        std::thread::spawn(move || {
            let mut stream = stream;
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).map(|_| bytes)
        })
    });

    let started = Instant::now();
    let status = loop {
        if control.is_cancelled() {
            terminate(&mut child);
            let _ = join_readers(stdout, stderr)?;
            return Err(NodeError::Cancelled);
        }
        if timeout.is_some_and(|limit| started.elapsed() >= limit) {
            terminate(&mut child);
            let _ = join_readers(stdout, stderr)?;
            return Err(NodeError::Timeout);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(POLL_INTERVAL),
            Err(error) => {
                terminate(&mut child);
                let _ = join_readers(stdout, stderr)?;
                return Err(NodeError::Io(error.to_string()));
            }
        }
    };

    let (stdout, stderr) = join_readers(stdout, stderr)?;
    Ok(ProcessResult {
        pid,
        exit_code: status.code(),
        stdout,
        stderr,
    })
}

fn build_command(config: &CommandConfig) -> Command {
    let mut command = if config.shell {
        #[cfg(windows)]
        {
            let mut command = Command::new("cmd");
            command.arg("/C").arg(&config.program).args(&config.args);
            command
        }
        #[cfg(not(windows))]
        {
            let rendered = std::iter::once(config.program.as_str())
                .chain(config.args.iter().map(String::as_str))
                .map(shell_quote)
                .collect::<Vec<_>>()
                .join(" ");
            let mut command = Command::new("sh");
            command.arg("-c").arg(rendered);
            command
        }
    } else {
        let mut command = Command::new(&config.program);
        command.args(&config.args);
        command
    };

    if let Some(cwd) = &config.cwd {
        command.current_dir(cwd);
    }
    command.envs(&config.env);
    command
}

#[cfg(not(windows))]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn terminate(child: &mut Child) {
    #[cfg(windows)]
    {
        // A shell can leave a descendant holding the stdout/stderr pipes.
        // Killing only `cmd.exe` would therefore make the reader threads wait
        // until that descendant exits. Terminate the whole process tree.
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
    let _ = child.wait();
}

type Reader = std::thread::JoinHandle<std::io::Result<Vec<u8>>>;

fn join_readers(stdout: Option<Reader>, stderr: Option<Reader>) -> NodeResult<(String, String)> {
    let stderr = match stderr {
        Some(handle) => handle
            .join()
            .map_err(|_| NodeError::Execution("stderr reader panicked".to_string()))?
            .map_err(|error| NodeError::Io(error.to_string()))?,
        None => Vec::new(),
    };
    let stdout = match stdout {
        Some(handle) => handle
            .join()
            .map_err(|_| NodeError::Execution("stdout reader panicked".to_string()))?
            .map_err(|error| NodeError::Io(error.to_string()))?,
        None => Vec::new(),
    };
    Ok((
        String::from_utf8_lossy(&stdout).into_owned(),
        String::from_utf8_lossy(&stderr).into_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell_command(program: &str) -> CommandConfig {
        CommandConfig {
            program: program.to_string(),
            args: Vec::new(),
            cwd: None,
            env: BTreeMap::new(),
            shell: true,
            stdin: None,
            check_exit_code: true,
            wait: true,
        }
    }

    #[test]
    fn direct_process_captures_stdout() {
        #[cfg(windows)]
        let config = CommandConfig {
            program: "cmd".to_string(),
            args: vec!["/C".to_string(), "echo hello".to_string()],
            cwd: None,
            env: BTreeMap::new(),
            shell: false,
            stdin: None,
            check_exit_code: true,
            wait: true,
        };
        #[cfg(not(windows))]
        let config = CommandConfig {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), "printf hello".to_string()],
            cwd: None,
            env: BTreeMap::new(),
            shell: false,
            stdin: None,
            check_exit_code: true,
            wait: true,
        };

        let result = run_process(&config, &RunControl::new(), None).unwrap();
        assert_eq!(result.stdout.trim(), "hello");
        assert_eq!(result.exit_code, Some(0));
    }

    #[test]
    fn shell_and_environment_are_applied() {
        #[cfg(windows)]
        let mut config = shell_command("echo %NODARA_TEST_VALUE%");
        #[cfg(not(windows))]
        let mut config = shell_command("printf \"$NODARA_TEST_VALUE\"");
        config
            .env
            .insert("NODARA_TEST_VALUE".to_string(), "configured".to_string());

        let result = run_process(&config, &RunControl::new(), None).unwrap();
        assert_eq!(result.stdout.trim(), "configured");
    }

    #[test]
    fn timeout_stops_a_long_running_process() {
        #[cfg(windows)]
        let config = shell_command("ping -n 6 127.0.0.1 >nul");
        #[cfg(not(windows))]
        let config = shell_command("sleep 2");

        let error = run_process(&config, &RunControl::new(), Some(Duration::from_millis(50)))
            .expect_err("command should time out");
        assert_eq!(error, NodeError::Timeout);
    }

    #[test]
    fn non_zero_exit_can_fail_or_be_inspected() {
        let mut config = shell_command("exit 3");
        let error = run_command(&config, &RunControl::new(), None)
            .expect_err("non-zero exit should fail by default");
        assert!(matches!(error, NodeError::Execution(message) if message.contains("code 3")));

        config.check_exit_code = false;
        let output = run_command(&config, &RunControl::new(), None).unwrap();
        assert_eq!(output.output("exit_code"), Some(&serde_json::json!(3)));
        assert_eq!(output.output("success"), Some(&serde_json::json!(false)));
    }
}
