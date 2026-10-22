import type { KeyboardEvent as ReactKeyboardEvent } from "react";

/** Keeps Tab and Shift+Tab inside a modal dialog. */
export function keepFocusInDialog(event: ReactKeyboardEvent<HTMLElement>) {
  if (event.key !== "Tab") return;
  const controls = Array.from(event.currentTarget.querySelectorAll<HTMLElement>(
    "button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled)"));
  if (controls.length === 0) return;
  if (event.shiftKey && document.activeElement === controls[0]) {
    event.preventDefault();
    controls[controls.length - 1].focus();
  } else if (!event.shiftKey && document.activeElement === controls[controls.length - 1]) {
    event.preventDefault();
    controls[0].focus();
  }
}
