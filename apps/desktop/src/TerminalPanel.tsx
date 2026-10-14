import { useEffect, useRef } from "react";
import "@xterm/xterm/css/xterm.css";
import type { Terminal as XtermTerminal } from "@xterm/xterm";
import type { TerminalOutput, TerminalSummary } from "./contracts";
import { executeCommand } from "./ipc";

interface TerminalPanelProps {
  theme: Record<string, string>;
  onClose: () => void;
  onStatus: (message: string) => void;
}

export function TerminalPanel({ theme, onClose, onStatus }: TerminalPanelProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<XtermTerminal | null>(null);
  const themeRef = useRef(theme);
  themeRef.current = theme;

  useEffect(() => {
    if (!terminalRef.current) return;
    terminalRef.current.options.theme = {
      background: theme["terminal.background"],
      foreground: theme["terminal.foreground"],
      cursor: theme["terminal.cursor"],
    };
  }, [theme]);

  useEffect(() => {
    let disposed = false;
    let terminalId: string | null = null;
    let cursor = "0";
    let reading = false;
    let terminal: XtermTerminal | null = null;
    let input: { dispose: () => void } | null = null;
    let resize: ResizeObserver | null = null;
    let timer: number | null = null;

    const report = (error: unknown, fallback: string) => {
      onStatus(error instanceof Error ? error.message : fallback);
    };
    const initialize = async () => {
      try {
        const [{ Terminal }, { FitAddon }] = await Promise.all([
          import("@xterm/xterm"), import("@xterm/addon-fit"),
        ]);
        if (disposed || !hostRef.current) return;
        const colors = themeRef.current;
        terminal = new Terminal({
          cursorBlink: true,
          fontFamily: 'ui-monospace, "SFMono-Regular", Consolas, monospace',
          fontSize: 12,
          scrollback: 5_000,
          theme: {
            background: colors["terminal.background"],
            foreground: colors["terminal.foreground"],
            cursor: colors["terminal.cursor"],
          },
        });
        terminalRef.current = terminal;
        const fit = new FitAddon();
        terminal.loadAddon(fit);
        terminal.open(hostRef.current);
        fit.fit();

        const created = await executeCommand<TerminalSummary>({
          id: "terminal.create", context: { windowId: "main" },
          arguments: { arguments: [], environment: {}, size: { columns: terminal.cols, rows: terminal.rows } },
        });
        if (disposed) {
          await executeCommand({ id: "terminal.close", context: { windowId: "main" }, arguments: { terminalId: created.terminalId } });
          return;
        }
        terminalId = created.terminalId;
        onStatus(`Started ${created.title}`);

        input = terminal.onData((data) => {
          if (!terminalId) return;
          void executeCommand({
            id: "terminal.write", context: { windowId: "main" },
            arguments: { terminalId, bytes: Array.from(new TextEncoder().encode(data)) },
          }).catch((error: unknown) => report(error, "Could not write to terminal"));
        });
        timer = window.setInterval(() => {
          if (!terminalId || reading || disposed || !terminal) return;
          reading = true;
          void executeCommand<TerminalOutput>({
            id: "terminal.read", context: { windowId: "main" },
            arguments: { terminalId, cursor, maximum: 65_536 },
          }).then((output) => {
            cursor = output.nextOffset;
            if (output.dropped) terminal?.write("\r\n[older terminal output was discarded]\r\n");
            if (output.bytes.length > 0) terminal?.write(new Uint8Array(output.bytes));
          }).catch((error: unknown) => report(error, "Could not read terminal output"))
            .finally(() => { reading = false; });
        }, 40);
        resize = new ResizeObserver(() => {
          fit.fit();
          if (!terminalId || !terminal) return;
          void executeCommand({
            id: "terminal.resize", context: { windowId: "main" },
            arguments: { terminalId, size: { columns: terminal.cols, rows: terminal.rows } },
          }).catch((error: unknown) => report(error, "Could not resize terminal"));
        });
        resize.observe(hostRef.current);
        terminal.focus();
      } catch (error: unknown) {
        report(error, "Could not start terminal");
      }
    };
    void initialize();

    return () => {
      disposed = true;
      if (timer !== null) window.clearInterval(timer);
      resize?.disconnect();
      input?.dispose();
      terminal?.dispose();
      terminalRef.current = null;
      if (terminalId) {
        void executeCommand({
          id: "terminal.close", context: { windowId: "main" }, arguments: { terminalId },
        });
      }
    };
  }, [onStatus]);

  return <section className="terminal-panel" aria-label="Integrated terminal">
    <header><span>TERMINAL</span><button type="button" aria-label="Close terminal" onClick={onClose}>×</button></header>
    <div className="terminal-host" ref={hostRef} />
  </section>;
}
