import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { CommandDescriptor, PluginCapability, PluginSummary } from "./contracts";
import { executeCommand, listCommands } from "./ipc";

interface PluginPanelProps {
  onClose(): void;
  onCommandsChange(commands: CommandDescriptor[]): void;
  onStatus(message: string): void;
}

function message(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}

/** Every family this plugin actually declares, in a stable order. */
function capabilitiesOf(plugin: PluginSummary): PluginCapability[] {
  const all = new Set<PluginCapability>();
  if (plugin.commands.length > 0) all.add("commands");
  for (const language of plugin.languages) {
    for (const capability of language.capabilities) all.add(capability);
  }
  const order: PluginCapability[] = [
    "commands", "languages", "highlight", "indentation", "completion", "snippets",
  ];
  return order.filter((capability) => all.has(capability));
}

export function PluginPanel({ onClose, onCommandsChange, onStatus }: PluginPanelProps) {
  const [plugins, setPlugins] = useState<PluginSummary[]>([]);
  const [busy, setBusy] = useState(false);

  const reload = async (refreshCommands = false) => {
    const next = await executeCommand<PluginSummary[]>({
      id: "plugin.list", context: { windowId: "main" }, arguments: {},
    });
    setPlugins(next);
    if (refreshCommands) onCommandsChange(await listCommands());
  };

  useEffect(() => {
    let mounted = true;
    void executeCommand<PluginSummary[]>({
      id: "plugin.list", context: { windowId: "main" }, arguments: {},
    }).then((next) => {
      if (mounted) setPlugins(next);
    }).catch((error: unknown) => {
      if (mounted) onStatus(message(error, "Could not load plugins"));
    });
    return () => { mounted = false; };
  }, []);

  /* Escape closes it. The panel announces itself aria-modal, so a keyboard user
     needs the dismiss key the app gives every other dialog. */
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const install = async () => {
    try {
      const selected = await open({ multiple: false, directory: true });
      if (typeof selected !== "string") return;
      setBusy(true);
      const installed = await executeCommand<PluginSummary>({
        id: "plugin.install", context: { windowId: "main" }, arguments: { path: selected },
      });
      await reload(true);
      onStatus(`Installed ${installed.name}; review and approve its permissions`);
    } catch (error: unknown) {
      onStatus(message(error, "Could not install plugin"));
    } finally {
      setBusy(false);
    }
  };

  const approve = async (plugin: PluginSummary) => {
    setBusy(true);
    try {
      await executeCommand({
        id: "plugin.approve", context: { windowId: "main" },
        arguments: { id: plugin.id, grant: { permissions: plugin.requestedPermissions } },
      });
      await reload(true);
      onStatus(`Approved ${plugin.name} for this exact package revision`);
    } catch (error: unknown) {
      onStatus(message(error, `Could not approve ${plugin.name}`));
    } finally {
      setBusy(false);
    }
  };

  const revoke = async (plugin: PluginSummary) => {
    setBusy(true);
    try {
      await executeCommand({
        id: "plugin.revoke", context: { windowId: "main" }, arguments: { id: plugin.id },
      });
      await reload(true);
      onStatus(`Revoked ${plugin.name}`);
    } catch (error: unknown) {
      onStatus(message(error, `Could not revoke ${plugin.name}`));
    } finally {
      setBusy(false);
    }
  };

  const uninstall = async (plugin: PluginSummary) => {
    setBusy(true);
    try {
      await executeCommand({
        id: "plugin.uninstall", context: { windowId: "main" }, arguments: { id: plugin.id },
      });
      await reload(true);
      onStatus(`Uninstalled ${plugin.name}`);
    } catch (error: unknown) {
      onStatus(message(error, `Could not uninstall ${plugin.name}`));
    } finally {
      setBusy(false);
    }
  };

  const setEnabled = async (plugin: PluginSummary, enabled: boolean) => {
    setBusy(true);
    try {
      await executeCommand({
        id: "plugin.setEnabled", context: { windowId: "main" },
        arguments: { id: plugin.id, enabled },
      });
      await reload(true);
      onStatus(`${enabled ? "Enabled" : "Disabled"} ${plugin.name}`);
    } catch (error: unknown) {
      onStatus(message(error, `Could not change ${plugin.name}`));
    } finally {
      setBusy(false);
    }
  };

  const setCapability = async (
    plugin: PluginSummary, capability: PluginCapability, enabled: boolean,
  ) => {
    setBusy(true);
    try {
      await executeCommand({
        id: "plugin.setCapabilityEnabled", context: { windowId: "main" },
        arguments: { id: plugin.id, capability, enabled },
      });
      await reload(true);
    } catch (error: unknown) {
      onStatus(message(error, `Could not change ${plugin.name}`));
    } finally {
      setBusy(false);
    }
  };

  return <section className="plugin-panel" role="dialog" aria-modal="true" aria-label="Plugins">
    <header>
      <div><span>EXTENSIONS</span><strong>Installed plugins</strong></div>
      <button type="button" aria-label="Close plugins" onClick={onClose}>×</button>
    </header>
    <div className="plugin-intro">
      <p>Plugins run in isolated worker processes or contribute validated language data. Turn one off to keep it installed without its commands, languages, or highlighting.</p>
      <button type="button" disabled={busy} onClick={() => void install()}>Install from folder…</button>
    </div>
    <div className="plugin-list" aria-label="Installed plugins">
      {plugins.map((plugin) => {
        const capabilities = capabilitiesOf(plugin);
        return <article key={plugin.id}>
          <div className="plugin-heading">
            <div><strong>{plugin.name}</strong><small>{plugin.id} · {plugin.version}</small></div>
            <span className={plugin.approved ? (plugin.enabled ? "approved" : "disabled") : "approval-required"}>
              {plugin.approved ? (plugin.enabled ? "Approved" : "Disabled") : "Approval required"}
            </span>
          </div>
          <dl>
            <div><dt>Permissions</dt><dd>{plugin.requestedPermissions.join(", ") || "None"}</dd></div>
            <div><dt>Commands</dt><dd>{plugin.commands.map((command) => command.command).join(", ") || "None"}</dd></div>
            <div><dt>Languages</dt><dd>{plugin.languages.map((language) => language.displayName).join(", ") || "None"}</dd></div>
            <div><dt>Revision</dt><dd title={plugin.digest}>{plugin.digest.slice(0, 12)}</dd></div>
          </dl>
          {plugin.approved && plugin.enabled && capabilities.length > 0
            ? <fieldset className="plugin-capabilities"><legend>Features</legend>
              {capabilities.map((capability) => <label key={capability}>
                <input type="checkbox" disabled={busy}
                  checked={!plugin.disabledCapabilities.includes(capability)}
                  aria-label={`${plugin.name} ${capability}`}
                  onChange={(event) => void setCapability(plugin, capability, event.target.checked)} />
                {capability}
              </label>)}
            </fieldset> : null}
          <div className="plugin-actions">
            {plugin.approved
              ? <button type="button" disabled={busy} onClick={() => void setEnabled(plugin, !plugin.enabled)}>
                {plugin.enabled ? "Disable" : "Enable"}
              </button>
              : <button className="primary" type="button" disabled={busy} onClick={() => void approve(plugin)}>Approve requested access</button>}
            {plugin.approved
              ? <button type="button" disabled={busy} onClick={() => void revoke(plugin)}>Revoke</button> : null}
            <button type="button" disabled={busy} aria-label={`Uninstall ${plugin.name}`}
              onClick={() => void uninstall(plugin)}>Uninstall</button>
          </div>
        </article>;
      })}
      {plugins.length === 0 ? <p>No plugins installed.</p> : null}
    </div>
  </section>;
}
