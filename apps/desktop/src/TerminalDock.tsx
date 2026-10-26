import { useEffect, useRef, useState, type RefObject } from "react";
import { TerminalPanel, type TerminalFont } from "./TerminalPanel";
import {
  TERMINAL_HEIGHT_STEP,
  TERMINAL_MIN_HEIGHT,
  clampTerminalHeight,
  loadTerminalHeight,
  saveTerminalHeight,
  terminalHeightFromDrag,
  terminalMaxHeight,
} from "./terminalLayout";

/** The terminal below the editor: whether it shows, whether its shell runs, and its height. */
export interface TerminalDockState {
  open: boolean;
  /** The shell runs from the first show until the panel's own close button. */
  alive: boolean;
  height: number;
  /** A divider drag is in progress (the workbench pins the resize cursor). */
  resizing: boolean;
  /** Bumped to hand the keyboard to the shell. */
  focusSignal: number;
  /** The editor column the panel and its divider sit in. */
  editorAreaRef: RefObject<HTMLElement>;
  editorAreaHeight: number;
  /** Shows the panel, starting the shell the first time, or hides it with the shell kept. */
  toggle(): void;
  /** Ends the shell and hides the panel. */
  close(): void;
  setHeight(update: number | ((height: number) => number)): void;
  startDrag(startY: number): void;
}

export function useTerminalDock(): TerminalDockState {
  const [open, setOpen] = useState(false);
  // Hiding the panel only folds it away; the shell keeps running behind the fold
  // and the same session comes back when the panel is shown again.
  const [alive, setAlive] = useState(false);
  const [height, setHeight] = useState(loadTerminalHeight);
  const [resizing, setResizing] = useState(false);
  const [focusSignal, setFocusSignal] = useState(0);
  const [editorAreaHeight, setEditorAreaHeight] = useState(0);
  const editorAreaRef = useRef<HTMLElement>(null);
  const dragRef = useRef<{ startHeight: number; startY: number; available: number } | null>(null);

  useEffect(() => {
    saveTerminalHeight(height);
  }, [height]);

  useEffect(() => {
    if (!resizing) return;
    // The divider is the panel's top edge, so the panel grows by the height the
    // pointer gives back on the way up. Bounds come from the column measured when
    // the drag began, which is the same box the divider moves inside.
    const onPointerMove = (event: PointerEvent) => {
      const drag = dragRef.current;
      if (!drag) return;
      setHeight(terminalHeightFromDrag(drag, event.clientY, drag.available));
    };
    const onPointerUp = () => {
      dragRef.current = null;
      setResizing(false);
      // The drag ends with the pointer over the panel, so typing should go there.
      setFocusSignal((nonce) => nonce + 1);
    };
    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp, { once: true });
    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
    };
  }, [resizing]);

  // The panel's bounds belong to the column it sits in: when the window shrinks
  // below the panel, the panel gives its height back and the editor keeps its strip.
  useEffect(() => {
    const area = editorAreaRef.current;
    if (!area || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver((entries) => {
      const available = Math.round(entries[0]?.contentRect.height ?? 0);
      setEditorAreaHeight(available);
      setHeight((current) => {
        const next = clampTerminalHeight(current, available);
        return next === current ? current : next;
      });
    });
    observer.observe(area);
    return () => observer.disconnect();
  }, []);

  return {
    open, alive, height, resizing, focusSignal, editorAreaRef, editorAreaHeight, setHeight,
    toggle() {
      if (open) {
        setOpen(false);
      } else {
        setAlive(true);
        setOpen(true);
        setFocusSignal((nonce) => nonce + 1);
      }
    },
    close() { setOpen(false); setAlive(false); },
    startDrag(startY) {
      dragRef.current = { startHeight: height, startY, available: editorAreaHeight };
      setResizing(true);
    },
  };
}

/** The divider and the panel, once a shell has been started. */
export function TerminalDock({ dock, theme, font, onStatus }: {
  dock: TerminalDockState; theme: Record<string, string>; font: TerminalFont; onStatus(message: string): void;
}) {
  if (!dock.alive) return null;
  const { editorAreaHeight, height } = dock;
  return <>
    {dock.open ? <div className="terminal-resizer" role="separator" tabIndex={0}
      aria-label="Resize terminal" aria-orientation="horizontal"
      aria-valuemin={TERMINAL_MIN_HEIGHT} aria-valuemax={terminalMaxHeight(editorAreaHeight)}
      aria-valuenow={height} aria-valuetext={`${height} pixels tall`}
      onPointerDown={(event) => {
        if (event.button !== 0) return;
        event.preventDefault();
        dock.startDrag(event.clientY);
      }}
      onKeyDown={(event) => {
        // Arrow keys lengthen the panel upward, the way dragging the divider does.
        const key = event.key;
        if (key === "ArrowUp") dock.setHeight((current) => clampTerminalHeight(current + TERMINAL_HEIGHT_STEP, editorAreaHeight));
        else if (key === "ArrowDown") dock.setHeight((current) => clampTerminalHeight(current - TERMINAL_HEIGHT_STEP, editorAreaHeight));
        else if (key === "Home") dock.setHeight(TERMINAL_MIN_HEIGHT);
        else if (key === "End") dock.setHeight(terminalMaxHeight(editorAreaHeight));
        else return;
        event.preventDefault();
      }} /> : null}
    <TerminalPanel theme={theme} font={font} height={height} hidden={!dock.open}
      focusSignal={dock.focusSignal}
      onClose={dock.close} onStatus={onStatus} />
  </>;
}
