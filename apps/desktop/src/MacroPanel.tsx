import { useEffect, useState } from "react";
import type {
  DocumentSummary, MacroDefinition, MacroRunOutcome, MacroSummary, RecordingStatus,
  SandboxCapability, SandboxPolicy, ScriptDefinition, ScriptRunOutcome, ScriptSummary, StoredScript,
} from "./contracts";
import { executeCommand } from "./ipc";

interface MacroPanelProps {
  activeDocument?: DocumentSummary;
  onClose(): void;
  onDocumentChange(document: DocumentSummary): void;
  onStatus(message: string): void;
}

const ID_PATTERN = /^[A-Za-z0-9._-]+\.[A-Za-z0-9._-]+$/;
const CAPABILITIES: SandboxCapability[] = [
  "readDocument", "editDocument", "readWorkspace", "writeWorkspace", "readSettings", "showMessage",
];
const DEFAULT_LIMITS = {
  memoryBytes: 16 * 1_048_576, executionMillis: 1_000, operations: 5_000_000,
  hostCalls: 1_000, outputBytes: 65_536,
};

function message(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

export function MacroPanel({ activeDocument, onClose, onDocumentChange, onStatus }: MacroPanelProps) {
  const [macros, setMacros] = useState<MacroSummary[]>([]);
  const [scripts, setScripts] = useState<ScriptSummary[]>([]);
  const [recording, setRecording] = useState<RecordingStatus>({
    recording: false, id: null, name: null, actionCount: 0, failed: null,
  });
  const [id, setId] = useState("user.macro");
  const [name, setName] = useState("My Macro");
  const [scriptId, setScriptId] = useState("user.script");
  const [scriptName, setScriptName] = useState("My Script");
  const [source, setSource] = useState('nulis_print("Hello from Nulis");');
  const [capabilities, setCapabilities] = useState<SandboxCapability[]>([]);
  const [commands, setCommands] = useState("");
  const [selectedScript, setSelectedScript] = useState<StoredScript>();
  const [scriptOutput, setScriptOutput] = useState("");
  const [busy, setBusy] = useState(false);
  const [scriptRunning, setScriptRunning] = useState(false);
  const context = { windowId: "main", documentId: activeDocument?.documentId };

  const reload = async () => {
    const [nextMacros, nextRecording, nextScripts] = await Promise.all([
      executeCommand<MacroSummary[]>({ id: "automation.listMacros", context: { windowId: "main" }, arguments: {} }),
      executeCommand<RecordingStatus>({ id: "automation.recordingStatus", context: { windowId: "main" }, arguments: {} }),
      executeCommand<ScriptSummary[]>({ id: "automation.listScripts", context: { windowId: "main" }, arguments: {} }),
    ]);
    setMacros(nextMacros); setRecording(nextRecording); setScripts(nextScripts);
  };

  useEffect(() => {
    let mounted = true;
    void reload().catch((error: unknown) => {
      if (mounted) onStatus(message(error, "Could not load automation"));
    });
    return () => { mounted = false; };
  }, []);

  const start = async () => {
    if (!ID_PATTERN.test(id) || !name.trim()) {
      onStatus("Use a namespaced macro ID and a non-empty name"); return;
    }
    setBusy(true);
    try {
      const next = await executeCommand<RecordingStatus>({
        id: "automation.startRecording", context, arguments: { id, name: name.trim() },
      });
      setRecording(next); onStatus(`Recording ${next.name ?? name}`);
    } catch (error: unknown) { onStatus(message(error, "Could not start macro recording")); }
    finally { setBusy(false); }
  };

  const stop = async () => {
    setBusy(true);
    try {
      const saved = await executeCommand<MacroDefinition>({ id: "automation.stopRecording", context, arguments: {} });
      await reload(); onStatus(`Saved ${saved.name} (${saved.actions.length} actions)`);
    } catch (error: unknown) { onStatus(message(error, "Could not save macro recording")); }
    finally { setBusy(false); }
  };

  const cancel = async () => {
    setBusy(true);
    try {
      const next = await executeCommand<RecordingStatus>({ id: "automation.cancelRecording", context, arguments: {} });
      setRecording(next); onStatus("Macro recording cancelled");
    } catch (error: unknown) { onStatus(message(error, "Could not cancel macro recording")); }
    finally { setBusy(false); }
  };

  const runMacro = async (macro: MacroSummary) => {
    if (!activeDocument) { onStatus("Open a document before running a macro"); return; }
    setBusy(true);
    try {
      const outcome = await executeCommand<MacroRunOutcome>({ id: "automation.runMacro", context, arguments: { id: macro.id } });
      if (outcome.document) onDocumentChange(outcome.document);
      onStatus(`Ran ${macro.name} (${outcome.actionsCompleted} actions)`);
    } catch (error: unknown) { onStatus(message(error, `Could not run ${macro.name}`)); }
    finally { setBusy(false); }
  };

  const removeMacro = async (macro: MacroSummary) => {
    setBusy(true);
    try {
      await executeCommand<boolean>({ id: "automation.deleteMacro", context: { windowId: "main" }, arguments: { id: macro.id } });
      await reload(); onStatus(`Deleted ${macro.name}`);
    } catch (error: unknown) { onStatus(message(error, `Could not delete ${macro.name}`)); }
    finally { setBusy(false); }
  };

  const currentDefinition = (): ScriptDefinition => {
    const requestedCommands = commands.split(",").map((command) => command.trim()).filter(Boolean);
    const requestedCapabilities = [...capabilities];
    if (requestedCommands.length && !requestedCapabilities.includes("executeCommand")) requestedCapabilities.push("executeCommand");
    return { apiVersion: 1, name: scriptName.trim(), source, requestedCapabilities, requestedCommands };
  };

  const saveScript = async () => {
    if (!ID_PATTERN.test(scriptId) || !scriptName.trim()) {
      onStatus("Use a namespaced script ID and a non-empty name"); return;
    }
    setBusy(true);
    try {
      const stored = await executeCommand<StoredScript>({
        id: "automation.saveScript", context, arguments: { id: scriptId, definition: currentDefinition() },
      });
      setSelectedScript(stored); await reload();
      onStatus(`Saved ${stored.definition.name}${stored.approvedPolicy ? "" : " (approval required)"}`);
    } catch (error: unknown) { onStatus(message(error, "Could not save script")); }
    finally { setBusy(false); }
  };

  const editScript = async (script: ScriptSummary) => {
    setBusy(true);
    try {
      const stored = await executeCommand<StoredScript>({ id: "automation.getScript", context, arguments: { id: script.id } });
      setSelectedScript(stored); setScriptId(stored.id); setScriptName(stored.definition.name);
      setSource(stored.definition.source); setCapabilities(stored.definition.requestedCapabilities);
      setCommands(stored.definition.requestedCommands.join(", "));
    } catch (error: unknown) { onStatus(message(error, `Could not load ${script.name}`)); }
    finally { setBusy(false); }
  };

  const approveScript = async () => {
    if (!selectedScript || selectedScript.id !== scriptId) { onStatus("Save the script before approving it"); return; }
    const policy: SandboxPolicy = {
      capabilities: selectedScript.definition.requestedCapabilities,
      allowedCommands: selectedScript.definition.requestedCommands,
      limits: DEFAULT_LIMITS,
    };
    setBusy(true);
    try {
      await executeCommand({ id: "automation.approveScript", context, arguments: { id: selectedScript.id, policy } });
      const approved = { ...selectedScript, approvedPolicy: policy };
      setSelectedScript(approved); await reload(); onStatus(`Approved ${approved.definition.name}`);
    } catch (error: unknown) { onStatus(message(error, "Could not approve script")); }
    finally { setBusy(false); }
  };

  const revokeScript = async () => {
    if (!selectedScript) return;
    setBusy(true);
    try {
      await executeCommand({ id: "automation.revokeScript", context, arguments: { id: selectedScript.id } });
      setSelectedScript({ ...selectedScript, approvedPolicy: null });
      await reload(); onStatus(`Revoked approval for ${selectedScript.definition.name}`);
    } catch (error: unknown) { onStatus(message(error, "Could not revoke script approval")); }
    finally { setBusy(false); }
  };

  const runScript = async (script: ScriptSummary) => {
    setScriptRunning(true); setScriptOutput("");
    try {
      const outcome = await executeCommand<ScriptRunOutcome>({ id: "automation.runScript", context, arguments: { id: script.id } });
      if (outcome.document) onDocumentChange(outcome.document);
      setScriptOutput([outcome.report.output.trimEnd(), ...outcome.messages].filter(Boolean).join("\n"));
      onStatus(`Ran ${script.name} (${outcome.report.hostCalls} host calls)`);
    } catch (error: unknown) { onStatus(message(error, `Could not run ${script.name}`)); }
    finally { setScriptRunning(false); }
  };

  const removeScript = async (script: ScriptSummary) => {
    setBusy(true);
    try {
      await executeCommand({ id: "automation.deleteScript", context: { windowId: "main" }, arguments: { id: script.id } });
      if (selectedScript?.id === script.id) setSelectedScript(undefined);
      await reload(); onStatus(`Deleted ${script.name}`);
    } catch (error: unknown) { onStatus(message(error, `Could not delete ${script.name}`)); }
    finally { setBusy(false); }
  };

  return <section className="macro-panel" aria-label="Automation">
    <header><strong>AUTOMATION</strong><button type="button" aria-label="Close macros" onClick={onClose}>×</button></header>
    <h3>MACROS</h3>
    {recording.recording ? <div className="macro-recording" role="status">
      <span className="recording-dot" aria-hidden="true" />
      <div><strong>{recording.name}</strong><small>{recording.failed ?? "Recording editor actions…"}</small></div>
      <button type="button" disabled={busy || Boolean(recording.failed)} onClick={() => void stop()}>Stop &amp; Save</button>
      <button type="button" disabled={busy} onClick={() => void cancel()}>Cancel</button>
    </div> : <div className="macro-form">
      <label><span>Name</span><input aria-label="Macro name" value={name} maxLength={128} onChange={(event) => setName(event.currentTarget.value)} /></label>
      <label><span>ID</span><input aria-label="Macro ID" value={id} maxLength={128} onChange={(event) => setId(event.currentTarget.value)} /></label>
      <button type="button" disabled={busy} onClick={() => void start()}>Start Recording</button>
    </div>}
    <div className="macro-list" aria-label="Saved macros">
      {macros.map((macro) => <div key={macro.id}>
        <div><strong>{macro.name}</strong><small>{macro.id} · {macro.actionCount} actions</small></div>
        <button type="button" disabled={busy || recording.recording || !activeDocument} aria-label={`Run ${macro.name}`} onClick={() => void runMacro(macro)}>Run</button>
        <button type="button" disabled={busy || recording.recording} aria-label={`Delete ${macro.name}`} onClick={() => void removeMacro(macro)}>×</button>
      </div>)}
      {macros.length === 0 ? <p>No saved macros yet.</p> : null}
    </div>

    <h3>SCRIPTS</h3>
    <div className="script-form">
      <div className="script-fields">
        <label><span>Name</span><input aria-label="Script name" value={scriptName} maxLength={128} onChange={(event) => setScriptName(event.currentTarget.value)} /></label>
        <label><span>ID</span><input aria-label="Script ID" value={scriptId} maxLength={128} onChange={(event) => setScriptId(event.currentTarget.value)} /></label>
      </div>
      <textarea aria-label="Script source" spellCheck={false} value={source} onChange={(event) => setSource(event.currentTarget.value)} />
      <fieldset><legend>Requested permissions</legend>
        {CAPABILITIES.map((capability) => <label key={capability}>
          <input type="checkbox" checked={capabilities.includes(capability)} onChange={(event) => {
            setCapabilities((current) => event.currentTarget.checked ? [...current, capability] : current.filter((item) => item !== capability));
          }} />{capability}</label>)}
      </fieldset>
      <label><span>Editor commands (comma-separated)</span><input aria-label="Requested commands" value={commands} onChange={(event) => setCommands(event.currentTarget.value)} /></label>
      <div className="script-actions">
        <button type="button" disabled={busy || scriptRunning} onClick={() => void saveScript()}>Save Script</button>
        <button type="button" disabled={busy || !selectedScript || Boolean(selectedScript.approvedPolicy)} onClick={() => void approveScript()}>Approve Requested Access</button>
        <button type="button" disabled={busy || !selectedScript?.approvedPolicy} onClick={() => void revokeScript()}>Revoke</button>
      </div>
    </div>
    <div className="macro-list script-list" aria-label="Saved scripts">
      {scripts.map((script) => <div key={script.id}>
        <button className="script-select" type="button" disabled={busy} onClick={() => void editScript(script)}>
          <strong>{script.name}</strong><small>{script.id} · {script.approved ? "approved" : "approval required"}</small>
        </button>
        <button type="button" disabled={busy || scriptRunning || !script.approved} aria-label={`Run script ${script.name}`} onClick={() => void runScript(script)}>Run</button>
        <button type="button" disabled={busy || scriptRunning} aria-label={`Delete script ${script.name}`} onClick={() => void removeScript(script)}>×</button>
      </div>)}
      {scripts.length === 0 ? <p>No saved scripts yet.</p> : null}
    </div>
    {scriptRunning ? <button className="cancel-script" type="button" onClick={() => {
      void executeCommand({ id: "automation.cancelScript", context, arguments: {} })
        .catch((error: unknown) => onStatus(message(error, "Could not cancel script")));
    }}>Cancel Running Script</button> : null}
    {scriptOutput ? <pre className="script-output" aria-label="Script output">{scriptOutput}</pre> : null}
  </section>;
}
