import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { open } from "@tauri-apps/plugin-dialog";
import { afterEach, expect, test, vi } from "vitest";
import type { ExecuteCommandRequest, PluginSummary } from "./contracts";
import { executeCommand, listCommands } from "./ipc";
import { PluginPanel } from "./PluginPanel";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("./ipc", () => ({ executeCommand: vi.fn(), listCommands: vi.fn() }));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

test("installs a package and requires an explicit revision-bound approval", async () => {
  const plugin: PluginSummary = {
    id: "nulis.hello-example", name: "Hello Nulis", version: "1.0.0", digest: "abcdef1234567890",
    apiVersion: 1, requestedPermissions: ["showMessage"], approved: false, enabled: true,
    disabledCapabilities: [], languages: [],
    commands: [{ command: "nulis.hello-example.hello", title: "Hello", category: "Examples" }],
  };
  let installed = false;
  vi.mocked(open).mockResolvedValue("/tmp/hello-plugin");
  vi.mocked(listCommands).mockResolvedValue([]);
  vi.mocked(executeCommand).mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "plugin.list") return (installed ? [plugin] : []) as never;
    if (request.id === "plugin.install") { installed = true; return plugin as never; }
    if (request.id === "plugin.approve") { plugin.approved = true; return null as never; }
    return null as never;
  });
  const onCommandsChange = vi.fn();
  render(<PluginPanel onClose={vi.fn()} onCommandsChange={onCommandsChange} onStatus={vi.fn()} />);
  await screen.findByText("No plugins installed.");

  fireEvent.click(screen.getByRole("button", { name: "Install from folder…" }));
  expect(await screen.findByText("Approval required")).toBeInTheDocument();
  expect(executeCommand).toHaveBeenCalledWith({
    id: "plugin.install", context: { windowId: "main" }, arguments: { path: "/tmp/hello-plugin" },
  });

  fireEvent.click(screen.getByRole("button", { name: "Approve requested access" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "plugin.approve", context: { windowId: "main" },
    arguments: { id: plugin.id, grant: { permissions: ["showMessage"] } },
  }));
  expect(await screen.findByText("Approved")).toBeInTheDocument();
  expect(onCommandsChange).toHaveBeenCalled();
});

test("revokes and uninstalls a plugin", async () => {
  const plugin: PluginSummary = {
    id: "nulis.hello-example", name: "Hello Nulis", version: "1.0.0", digest: "abcdef1234567890",
    apiVersion: 1, requestedPermissions: ["showMessage"], approved: true, enabled: true,
    disabledCapabilities: [], languages: [], commands: [],
  };
  let present = true;
  vi.mocked(listCommands).mockResolvedValue([]);
  vi.mocked(executeCommand).mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "plugin.list") return (present ? [plugin] : []) as never;
    if (request.id === "plugin.revoke") { plugin.approved = false; return true as never; }
    if (request.id === "plugin.uninstall") { present = false; return true as never; }
    return null as never;
  });
  render(<PluginPanel onClose={vi.fn()} onCommandsChange={vi.fn()} onStatus={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "Revoke" }));
  expect(await screen.findByText("Approval required")).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Uninstall Hello Nulis" }));
  expect(await screen.findByText("No plugins installed.")).toBeInTheDocument();
});

test("disables a plugin and one declarative capability without revoking it", async () => {
  const plugin: PluginSummary = {
    id: "acme.lang-nim", name: "Nim", version: "2.0.0", digest: "abcdef1234567890",
    apiVersion: 2, requestedPermissions: [], approved: true, enabled: true,
    disabledCapabilities: [], commands: [],
    languages: [{
      id: "nim", displayName: "Nim", extensions: ["nim"], filenames: [],
      capabilities: ["languages", "highlight", "indentation"],
    }],
  };
  vi.mocked(executeCommand).mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "plugin.list") return [plugin] as never;
    return true as never;
  });
  render(<PluginPanel onClose={vi.fn()} onCommandsChange={vi.fn()} onStatus={vi.fn()} />);

  fireEvent.click(await screen.findByRole("checkbox", { name: "Nim highlight" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "plugin.setCapabilityEnabled", context: { windowId: "main" },
    arguments: { id: "acme.lang-nim", capability: "highlight", enabled: false },
  }));

  fireEvent.click(screen.getByRole("button", { name: "Disable" }));
  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "plugin.setEnabled", context: { windowId: "main" },
    arguments: { id: "acme.lang-nim", enabled: false },
  }));
});

test("closes on Escape, the key every other aria-modal dialog answers", async () => {
  vi.mocked(executeCommand).mockResolvedValue([] as never);
  const onClose = vi.fn();
  render(<PluginPanel onClose={onClose} onCommandsChange={vi.fn()} onStatus={vi.fn()} />);
  await screen.findByText("No plugins installed.");

  fireEvent.keyDown(window, { key: "Escape" });
  expect(onClose).toHaveBeenCalledTimes(1);

  fireEvent.keyDown(window, { key: "Enter" });
  expect(onClose).toHaveBeenCalledTimes(1);
});
