use crate::{
    PluginError, PluginHostOperation, PluginWorkerRequest, PluginWorkerResponse,
    MAX_PLUGIN_RESPONSE_BYTES, PLUGIN_API_VERSION,
};
use boa_engine::{Context, Source};
use serde_json::Value;
use std::{
    fmt::Write as _,
    io::{BufRead, Read, Write},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const WORKER_ARGUMENT: &str = "--nulis-plugin-worker";
const WORKER_MESSAGE_BYTES: u64 = 2 * 1_048_576;
const WORKER_POLL_MILLIS: u64 = 10;
const WORKER_EXECUTION_MILLIS: u64 = 2_000;
const LOOP_ITERATION_LIMIT: u64 = 1_000_000;
const RECURSION_LIMIT: usize = 128;
const STACK_SIZE_LIMIT: usize = 1_024;
#[cfg(target_os = "linux")]
const WORKER_ADDRESS_SPACE_BYTES: u64 = 512 * 1024 * 1024;

/// Executes one plugin request inside Boa without Node, Web APIs, module
/// loading, or host functions. The plugin can only return bounded JSON and
/// request the serializable operations exposed by the bootstrap below.
///
/// # Errors
/// Returns a validation, JavaScript, or response-size error.
pub fn execute_worker_request(
    request: &PluginWorkerRequest,
) -> Result<PluginWorkerResponse, PluginError> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        execute_worker_request_inner(request)
    }))
    .unwrap_or_else(|_| {
        Err(PluginError::Execution(
            "JavaScript runtime limit exceeded".into(),
        ))
    })
}

fn execute_worker_request_inner(
    request: &PluginWorkerRequest,
) -> Result<PluginWorkerResponse, PluginError> {
    request.validate()?;
    let arguments = serde_json::to_string(&request.arguments)
        .map_err(|error| PluginError::Malformed(error.to_string()))?;
    let command = serde_json::to_string(&request.command)
        .map_err(|error| PluginError::Malformed(error.to_string()))?;
    let mut program = plugin_bootstrap();
    program.push('\n');
    program.push_str(&request.source);
    program.push('\n');
    let _ = write!(
        program,
        r#"
globalThis.__nulisComplete = null;
Promise.resolve()
  .then(() => {{
    if (typeof activate !== "function") throw new Error("plugin must define activate(nulis, context)");
    return activate(globalThis.__nulisApi, globalThis.__nulisContext);
  }})
  .then(() => {{
    const command = {command};
    if (command === null) return null;
    const handler = globalThis.__nulisHandlers[command];
    if (typeof handler !== "function") throw new Error(`command was not registered: ${{command}}`);
    return handler(...{arguments});
  }})
  .then((result) => {{
    globalThis.__nulisComplete = JSON.stringify({{
      ok: true,
      result: result === undefined ? null : result,
      operations: globalThis.__nulisOperations,
      registeredCommands: Object.keys(globalThis.__nulisHandlers).sort()
    }});
  }})
  .catch((error) => {{
    globalThis.__nulisComplete = JSON.stringify({{ ok: false, error: String(error) }});
  }});
"#
    );

    let mut context = Context::default();
    context
        .runtime_limits_mut()
        .set_loop_iteration_limit(LOOP_ITERATION_LIMIT);
    context
        .runtime_limits_mut()
        .set_recursion_limit(RECURSION_LIMIT);
    context
        .runtime_limits_mut()
        .set_stack_size_limit(STACK_SIZE_LIMIT);
    context
        .eval(Source::from_bytes(&program))
        .map_err(|error| PluginError::Execution(error.to_string()))?;
    context.run_jobs();
    let encoded = context
        .eval(Source::from_bytes("globalThis.__nulisComplete"))
        .map_err(|error| PluginError::Execution(error.to_string()))?
        .to_string(&mut context)
        .map_err(|error| PluginError::Execution(error.to_string()))?
        .to_std_string_escaped();
    if encoded.len() > MAX_PLUGIN_RESPONSE_BYTES {
        return Err(PluginError::LimitExceeded {
            resource: "pluginResponseBytes".into(),
            limit: MAX_PLUGIN_RESPONSE_BYTES as u64,
        });
    }
    let envelope: WorkerEnvelope = serde_json::from_str(&encoded)
        .map_err(|error| PluginError::Malformed(error.to_string()))?;
    if !envelope.ok {
        return Err(PluginError::Execution(
            envelope.error.unwrap_or_else(|| "plugin failed".into()),
        ));
    }
    let response = PluginWorkerResponse {
        result: envelope.result,
        operations: envelope.operations,
        registered_commands: envelope.registered_commands,
    };
    response.validate()?;
    Ok(response)
}

fn plugin_bootstrap() -> String {
    format!(
        r#""use strict";
globalThis.__nulisOperations = [];
globalThis.__nulisHandlers = Object.create(null);
globalThis.__nulisContext = Object.freeze({{ subscriptions: [] }});
globalThis.__nulisApi = Object.freeze({{
  apiVersion: {PLUGIN_API_VERSION},
  commands: Object.freeze({{
    registerCommand(id, handler) {{
      if (typeof id !== "string" || typeof handler !== "function") throw new TypeError("invalid command registration");
      if (Object.prototype.hasOwnProperty.call(globalThis.__nulisHandlers, id)) throw new Error(`duplicate command: ${{id}}`);
      globalThis.__nulisHandlers[id] = handler;
      return Object.freeze({{ dispose() {{ delete globalThis.__nulisHandlers[id]; }} }});
    }}
  }}),
  window: Object.freeze({{
    showInformationMessage(message) {{
      if (typeof message !== "string") throw new TypeError("message must be a string");
      globalThis.__nulisOperations.push({{ kind: "showMessage", message }});
      return Promise.resolve();
    }}
  }})
}});
"#
    )
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct WorkerEnvelope {
    ok: bool,
    #[serde(default)]
    result: Value,
    #[serde(default)]
    operations: Vec<PluginHostOperation>,
    #[serde(default)]
    registered_commands: Vec<String>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ProcessPluginRunner {
    executable: std::path::PathBuf,
    execution_millis: u64,
}

impl ProcessPluginRunner {
    #[must_use]
    pub fn new(executable: std::path::PathBuf) -> Self {
        Self {
            executable,
            execution_millis: WORKER_EXECUTION_MILLIS,
        }
    }

    #[must_use]
    pub fn with_execution_millis(mut self, execution_millis: u64) -> Self {
        self.execution_millis = execution_millis.clamp(1, 10_000);
        self
    }

    /// Runs one request in a fresh child process. Cancellation, timeout, a
    /// protocol violation, or any read failure kills the worker.
    ///
    /// # Errors
    /// Returns a typed process, timeout, cancellation, or worker error.
    pub fn run<Cancelled>(
        &self,
        request: &PluginWorkerRequest,
        mut is_cancelled: Cancelled,
    ) -> Result<PluginWorkerResponse, PluginError>
    where
        Cancelled: FnMut() -> bool,
    {
        request.validate()?;
        let mut child = Command::new(&self.executable)
            .arg(WORKER_ARGUMENT)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(PluginError::io)?;
        let mut stdin = child.stdin.take().ok_or(PluginError::WorkerProtocol)?;
        serde_json::to_writer(&mut stdin, request)
            .map_err(|error| PluginError::Malformed(error.to_string()))?;
        stdin.write_all(b"\n").map_err(PluginError::io)?;
        drop(stdin);

        let stdout = child.stdout.take().ok_or(PluginError::WorkerProtocol)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let mut output = String::new();
            let result = stdout
                .take(WORKER_MESSAGE_BYTES + 1)
                .read_to_string(&mut output)
                .map(|_| output);
            let _ = sender.send(result);
        });
        let started = Instant::now();
        loop {
            if is_cancelled() {
                let _ = child.kill();
                let _ = child.wait();
                return Err(PluginError::Cancelled);
            }
            if started.elapsed() >= Duration::from_millis(self.execution_millis) {
                let _ = child.kill();
                let _ = child.wait();
                return Err(PluginError::TimedOut(self.execution_millis));
            }
            match receiver.recv_timeout(Duration::from_millis(WORKER_POLL_MILLIS)) {
                Ok(Ok(output)) => {
                    let status = child.wait().map_err(PluginError::io)?;
                    if !status.success() || output.len() as u64 > WORKER_MESSAGE_BYTES {
                        return Err(PluginError::WorkerProtocol);
                    }
                    let response: Result<PluginWorkerResponse, PluginError> =
                        serde_json::from_str(&output)
                            .map_err(|error| PluginError::Malformed(error.to_string()))?;
                    return response;
                }
                Ok(Err(error)) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(PluginError::io(error));
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(PluginError::WorkerProtocol);
                }
            }
        }
    }
}

/// Handles one bounded request on stdin and writes exactly one JSON result.
/// This is called before the desktop event loop starts in worker mode.
///
/// # Errors
/// Returns an I/O error only when the worker transport itself is unavailable.
pub fn run_worker_stdio() -> std::io::Result<()> {
    #[cfg(target_os = "linux")]
    {
        use rustix::process::{getrlimit, setrlimit, Resource, Rlimit};
        let resource = Resource::As;
        let existing = getrlimit(resource);
        let current = existing
            .current
            .unwrap_or(WORKER_ADDRESS_SPACE_BYTES)
            .min(WORKER_ADDRESS_SPACE_BYTES);
        setrlimit(
            resource,
            Rlimit {
                current: Some(current),
                maximum: existing.maximum,
            },
        )?;
    }
    let mut input = String::new();
    std::io::stdin()
        .lock()
        .take(WORKER_MESSAGE_BYTES + 1)
        .read_line(&mut input)?;
    let result = if input.len() as u64 > WORKER_MESSAGE_BYTES {
        Err(PluginError::WorkerProtocol)
    } else {
        serde_json::from_str::<PluginWorkerRequest>(&input)
            .map_err(|error| PluginError::Malformed(error.to_string()))
            .and_then(|request| execute_worker_request(&request))
    };
    let mut stdout = std::io::stdout().lock();
    serde_json::to_writer(&mut stdout, &result)?;
    stdout.write_all(b"\n")?;
    stdout.flush()
}

#[cfg(test)]
mod tests {
    use super::execute_worker_request;
    use crate::{PluginHostOperation, PluginWorkerRequest};
    use serde_json::json;

    #[test]
    fn javascript_worker_registers_invokes_and_collects_only_host_messages() {
        let response = execute_worker_request(&PluginWorkerRequest {
            api_version: 1,
            source: r#"
function activate(nulis, context) {
  context.subscriptions.push(nulis.commands.registerCommand("nulis.example.hello", async (name) => {
    await nulis.window.showInformationMessage(`Hello ${name}`);
    return { greeted: name };
  }));
}"#
            .into(),
            command: Some("nulis.example.hello".into()),
            arguments: vec![json!("Nulis")],
        })
        .expect("execute plugin");
        assert_eq!(response.result, json!({ "greeted": "Nulis" }));
        assert_eq!(
            response.operations,
            vec![PluginHostOperation::ShowMessage {
                message: "Hello Nulis".into()
            }]
        );
        assert_eq!(response.registered_commands, ["nulis.example.hello"]);
    }

    #[test]
    fn worker_rejects_missing_commands_and_runaway_loops() {
        let missing = execute_worker_request(&PluginWorkerRequest {
            api_version: 1,
            source: "function activate() {}".into(),
            command: Some("nulis.example.missing".into()),
            arguments: Vec::new(),
        });
        assert!(missing.is_err());
        let runaway = execute_worker_request(&PluginWorkerRequest {
            api_version: 1,
            source: "function activate() { while (true) {} }".into(),
            command: None,
            arguments: Vec::new(),
        });
        assert!(runaway.is_err());
    }
}
