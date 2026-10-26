import { useEffect, useRef, useState } from "react";
import "@xterm/xterm/css/xterm.css";
import type { Terminal as XtermTerminal } from "@xterm/xterm";
import type { TerminalOutput, TerminalSummary } from "./contracts";
import { adoptStyleElements } from "./adoptedStyles";
import { executeCommand } from "./ipc";

export interface TerminalFont {
  family: string;
  size: number;
  /** The editor's line-height multiplier; a terminal keeps its rows tight. */
  lineHeight: number;
}

interface TerminalPanelProps {
  theme: Record<string, string>;
  font: TerminalFont;
  /** The panel's height in CSS pixels, set by the divider above it. */
  height: number;
  /** Hidden keeps the shell alive: only closing it ends the session. */
  hidden: boolean;
  /** Raised when the panel should take keyboard focus back, such as after a divider drag. */
  focusSignal: number;
  onClose: () => void;
  onStatus: (message: string) => void;
}

/** Line spacing wide enough to read, narrow enough that a short panel shows a prompt. */
const MAX_LINE_HEIGHT = 1.25;
/** Below this the host is a collapsed panel, not a terminal worth fitting. */
const MIN_FIT_SIZE = 24;

function terminalLineHeight(lineHeight: number): number {
  return Number.isFinite(lineHeight) ? Math.min(MAX_LINE_HEIGHT, Math.max(1, lineHeight)) : 1;
}

export function TerminalPanel({ theme, font, height, hidden, focusSignal, onClose, onStatus }: TerminalPanelProps) {
  const hostRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<XtermTerminal | null>(null);
  const fitRef = useRef<{ fit: () => void } | null>(null);
  const sessionRef = useRef<string | null>(null);
  const themeRef = useRef(theme);
  themeRef.current = theme;
  const fontRef = useRef(font);
  fontRef.current = font;
  const hiddenRef = useRef(hidden);
  hiddenRef.current = hidden;
  const [focused, setFocused] = useState(false);

  useEffect(() => {
    if (!terminalRef.current) return;
    terminalRef.current.options.theme = {
      background: theme["terminal.background"],
      foreground: theme["terminal.foreground"],
      cursor: theme["terminal.cursor"],
      // A block cursor paints over the glyph it covers, so the glyph needs the
      // panel's own ground colour to stay readable under it.
      cursorAccent: theme["terminal.background"],
    };
  }, [theme]);

  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal) return;
    terminal.options.fontFamily = font.family;
    terminal.options.fontSize = font.size;
    terminal.options.lineHeight = terminalLineHeight(font.lineHeight);
  }, [font]);

  useEffect(() => {
    let disposed = false;
    let terminalId: string | null = null;
    let cursor = "0";
    let reading = false;
    let terminal: XtermTerminal | null = null;
    let input: { dispose: () => void } | null = null;
    let focusEvents: { dispose: () => void } | null = null;
    let releaseStyles: (() => void) | null = null;
    let resize: ResizeObserver | null = null;
    let timer: number | null = null;

    const report = (error: unknown, fallback: string) => {
      onStatus(error instanceof Error ? error.message : fallback);
    };
    const sendResize = () => {
      if (!terminalId || !terminal) return;
      void executeCommand({
        id: "terminal.resize", context: { windowId: "main" },
        arguments: { terminalId, size: { columns: terminal.cols, rows: terminal.rows } },
      }).catch((error: unknown) => report(error, "Could not resize terminal"));
    };
    const initialize = async () => {
      try {
        const [{ Terminal }, { FitAddon }] = await Promise.all([
          import("@xterm/xterm"), import("@xterm/addon-fit"),
        ]);
        if (disposed || !hostRef.current) return;
        const colors = themeRef.current;
        const face = fontRef.current;
        terminal = new Terminal({
          cursorBlink: true,
          cursorStyle: "block",
          // An unfocused terminal shows an outline, so it never claims to be the
          // place keystrokes go.
          cursorInactiveStyle: "outline",
          fontFamily: face.family,
          fontSize: face.size,
          lineHeight: terminalLineHeight(face.lineHeight),
          letterSpacing: 0,
          scrollback: 5_000,
          scrollOnUserInput: true,
          // xterm lifts the text's contrast against the panel ground up to this ratio,
          // so a theme that pairs a dim foreground with a dark panel stays readable.
          minimumContrastRatio: 4,
          theme: {
            background: colors["terminal.background"],
            foreground: colors["terminal.foreground"],
            cursor: colors["terminal.cursor"],
            cursorAccent: colors["terminal.background"],
          },
        });
        terminalRef.current = terminal;
        const fit = new FitAddon();
        terminal.loadAddon(fit);
        fitRef.current = fit;
        // xterm writes its font, cell and cursor rules into <style> elements, which the
        // app's CSP blocks in a packaged build; mirror them into adopted stylesheets.
        releaseStyles = adoptStyleElements(hostRef.current);
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
        sessionRef.current = created.terminalId;
        onStatus(`Started ${created.title}`);

        input = terminal.onData((data) => {
          if (!terminalId) return;
          void executeCommand({
            id: "terminal.write", context: { windowId: "main" },
            arguments: { terminalId, bytes: Array.from(new TextEncoder().encode(data)) },
          }).catch((error: unknown) => report(error, "Could not write to terminal"));
        });
        // xterm 6 removed its focus events, so the panel watches the DOM focus of the
        // element xterm routes keystrokes through. Keying off activeElement keeps the
        // reading right when focus moves around inside the panel.
        const host = hostRef.current;
        const syncFocus = () => setFocused(host.contains(document.activeElement));
        host.addEventListener("focusin", syncFocus);
        host.addEventListener("focusout", syncFocus);
        syncFocus();
        focusEvents = {
          dispose: () => {
            host.removeEventListener("focusin", syncFocus);
            host.removeEventListener("focusout", syncFocus);
          },
        };
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
        if (typeof ResizeObserver !== "undefined") {
          resize = new ResizeObserver(() => {
            const host = hostRef.current;
            // A hidden panel has no box to fit. Fitting it then would squeeze the
            // live shell to two columns and reflow the very output it is running.
            if (!host || !fit || host.clientWidth < MIN_FIT_SIZE || host.clientHeight < MIN_FIT_SIZE) return;
            fit.fit();
            sendResize();
          });
          resize.observe(hostRef.current);
        }
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
      focusEvents?.dispose();
      terminal?.dispose();
      releaseStyles?.();
      terminalRef.current = null;
      fitRef.current = null;
      sessionRef.current = null;
      if (terminalId) {
        void executeCommand({
          id: "terminal.close", context: { windowId: "main" }, arguments: { terminalId },
        });
      }
    };
  }, [onStatus]);

  // Showing the panel again gives it a real box, so the grid is re-measured and the
  // live shell is told the new size. Typing resumes where the prompt left it.
  useEffect(() => {
    const terminal = terminalRef.current;
    const fit = fitRef.current;
    if (hidden || !terminal || !fit) return;
    const host = hostRef.current;
    if (!host || host.clientWidth < MIN_FIT_SIZE || host.clientHeight < MIN_FIT_SIZE) return;
    fit.fit();
    const terminalId = sessionRef.current;
    if (!terminalId) return;
    void executeCommand({
      id: "terminal.resize", context: { windowId: "main" },
      arguments: { terminalId, size: { columns: terminal.cols, rows: terminal.rows } },
    }).catch(() => undefined);
  }, [hidden]);

  useEffect(() => {
    if (focusSignal > 0) terminalRef.current?.focus();
  }, [focusSignal]);

  return <section className={`terminal-panel${focused ? " focused" : ""}`} aria-label="Integrated terminal" hidden={hidden}
    style={{ height }}>
    <header><span>TERMINAL</span><button type="button" aria-label="Close terminal" onClick={onClose}>×</button></header>
    <div className="terminal-body" onPointerDown={() => terminalRef.current?.focus()}>
      <div className="terminal-host" ref={hostRef} />
    </div>
  </section>;
}
