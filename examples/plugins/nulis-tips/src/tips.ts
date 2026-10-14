/** Editor tips shown by the plugin. Add a line to add a tip. */
export const TIPS: readonly string[] = [
  "Press Ctrl/⌘ K to run any command by name.",
  "Hold Alt and click to add a second caret.",
  "Hold Alt + Shift and drag to select a rectangle of text.",
  "Drag a tab into the editor area to view two files side by side.",
  "Press Ctrl/⌘ D to select the word at the caret.",
  "Press Ctrl/⌘ G to go to a line, even in a very large file.",
  "A red bar in the gutter marks an unsaved line; green marks a saved one.",
  "Press F1 to open Nulis Help.",
];

/**
 * Picks a tip. A whole number chooses that tip (wrapping around); anything
 * else chooses the tip of the day.
 */
export function pickTip(index: unknown, today: Date = new Date()): { index: number; text: string } {
  const chosen = typeof index === "number" && Number.isInteger(index)
    ? index
    : Math.floor(today.getTime() / 86_400_000);
  const wrapped = ((chosen % TIPS.length) + TIPS.length) % TIPS.length;
  return { index: wrapped, text: TIPS[wrapped] };
}
