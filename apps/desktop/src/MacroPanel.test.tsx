import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import "@testing-library/jest-dom/vitest";
import { afterEach, expect, test, vi } from "vitest";
import type { ExecuteCommandRequest } from "./contracts";
import { executeCommand } from "./ipc";
import { MacroPanel } from "./MacroPanel";

vi.mock("./ipc", () => ({ executeCommand: vi.fn() }));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

test("records and atomically saves a named macro", async () => {
  const lists = [[], [{ id: "user.insert-x", name: "Insert X", actionCount: 1 }]];
  vi.mocked(executeCommand).mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "automation.listMacros") return lists.shift() as never;
    if (request.id === "automation.listScripts") return [] as never;
    if (request.id === "automation.recordingStatus") {
      return { recording: false, id: null, name: null, actionCount: 0, failed: null } as never;
    }
    if (request.id === "automation.startRecording") {
      return {
        recording: true, id: "user.insert-x", name: "Insert X", actionCount: 0, failed: null,
      } as never;
    }
    if (request.id === "automation.stopRecording") {
      return {
        schemaVersion: 1, id: "user.insert-x", name: "Insert X",
        actions: [{ command: "editor.insertText", arguments: {} }],
      } as never;
    }
    return null as never;
  });
  const onStatus = vi.fn();
  render(<MacroPanel onClose={vi.fn()} onDocumentChange={vi.fn()} onStatus={onStatus} />);
  await screen.findByText("No saved macros yet.");
  fireEvent.change(screen.getByRole("textbox", { name: "Macro name" }), {
    target: { value: "Insert X" },
  });
  fireEvent.change(screen.getByRole("textbox", { name: "Macro ID" }), {
    target: { value: "user.insert-x" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Start Recording" }));

  expect(await screen.findByText("Recording editor actions…")).toBeInTheDocument();
  expect(executeCommand).toHaveBeenCalledWith({
    id: "automation.startRecording",
    context: { windowId: "main", documentId: undefined },
    arguments: { id: "user.insert-x", name: "Insert X" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Stop & Save" }));
  expect(await screen.findByText("user.insert-x · 1 actions")).toBeInTheDocument();
  expect(onStatus).toHaveBeenLastCalledWith("Saved Insert X (1 actions)");
});

test("runs a saved macro in the active document context", async () => {
  vi.mocked(executeCommand).mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "automation.listMacros") {
      return [{ id: "user.insert-x", name: "Insert X", actionCount: 1 }] as never;
    }
    if (request.id === "automation.recordingStatus") {
      return { recording: false, id: null, name: null, actionCount: 0, failed: null } as never;
    }
    if (request.id === "automation.listScripts") return [] as never;
    if (request.id === "automation.runMacro") {
      return {
        actionsCompleted: 1,
        document: {
          documentId: "document-1", name: "Test", byteLen: "1", revision: "1", encoding: "utf8",
        },
      } as never;
    }
    return null as never;
  });
  const onDocumentChange = vi.fn();
  render(<MacroPanel
    activeDocument={{
      documentId: "document-1", name: "Test", byteLen: "0", revision: "0", encoding: "utf8",
    }}
    onClose={vi.fn()} onDocumentChange={onDocumentChange} onStatus={vi.fn()} />);
  fireEvent.click(await screen.findByRole("button", { name: "Run Insert X" }));

  await waitFor(() => expect(executeCommand).toHaveBeenCalledWith({
    id: "automation.runMacro",
    context: { windowId: "main", documentId: "document-1" },
    arguments: { id: "user.insert-x" },
  }));
  expect(onDocumentChange).toHaveBeenCalledWith(expect.objectContaining({ revision: "1" }));
});

test("saves, explicitly approves, and runs a sandboxed script", async () => {
  const script = {
    id: "user.hello",
    definition: {
      apiVersion: 1, name: "Hello", source: 'nulis_print("hello");',
      requestedCapabilities: [], requestedCommands: [],
    },
    approvedPolicy: null,
  };
  let saved = false;
  let approved = false;
  vi.mocked(executeCommand).mockImplementation(async (request: ExecuteCommandRequest) => {
    if (request.id === "automation.listMacros") return [] as never;
    if (request.id === "automation.recordingStatus") {
      return { recording: false, id: null, name: null, actionCount: 0, failed: null } as never;
    }
    if (request.id === "automation.listScripts") {
      return saved ? [{
        id: script.id, name: script.definition.name, requestedCapabilities: [],
        requestedCommands: [], approved,
      }] as never : [] as never;
    }
    if (request.id === "automation.saveScript") { saved = true; return script as never; }
    if (request.id === "automation.approveScript") { approved = true; return null as never; }
    if (request.id === "automation.runScript") {
      return { report: { operations: 1, hostCalls: 1, output: "hello\n" }, document: null, messages: [] } as never;
    }
    return null as never;
  });
  const onStatus = vi.fn();
  render(<MacroPanel onClose={vi.fn()} onDocumentChange={vi.fn()} onStatus={onStatus} />);
  await screen.findByText("No saved scripts yet.");
  fireEvent.change(screen.getByRole("textbox", { name: "Script name" }), { target: { value: "Hello" } });
  fireEvent.change(screen.getByRole("textbox", { name: "Script ID" }), { target: { value: "user.hello" } });
  fireEvent.change(screen.getByRole("textbox", { name: "Script source" }), { target: { value: 'nulis_print("hello");' } });
  fireEvent.click(screen.getByRole("button", { name: "Save Script" }));
  await screen.findByText("user.hello · approval required");
  fireEvent.click(screen.getByRole("button", { name: "Approve Requested Access" }));
  const run = await screen.findByRole("button", { name: "Run script Hello" });
  await waitFor(() => expect(run).toBeEnabled());
  fireEvent.click(run);
  expect(await screen.findByLabelText("Script output")).toHaveTextContent("hello");
});
