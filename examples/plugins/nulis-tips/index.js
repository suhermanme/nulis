"use strict";
(() => {
  // src/tips.ts
  var TIPS = [
    "Press Ctrl/⌘ K to run any command by name.",
    "Hold Alt and click to add a second caret.",
    "Hold Alt + Shift and drag to select a rectangle of text.",
    "Drag a tab into the editor area to view two files side by side.",
    "Press Ctrl/⌘ D to select the word at the caret.",
    "Press Ctrl/⌘ G to go to a line, even in a very large file.",
    "A red bar in the gutter marks an unsaved line; green marks a saved one.",
    "Press F1 to open Nulis Help."
  ];
  function pickTip(index, today = /* @__PURE__ */ new Date()) {
    const chosen = typeof index === "number" && Number.isInteger(index) ? index : Math.floor(today.getTime() / 864e5);
    const wrapped = (chosen % TIPS.length + TIPS.length) % TIPS.length;
    return { index: wrapped, text: TIPS[wrapped] };
  }

  // src/index.ts
  function requestedIndex(argument) {
    return argument !== null && typeof argument === "object" && !Array.isArray(argument) ? argument.index : void 0;
  }
  var activate = (nulis, context) => {
    void nulis.window.showInformationMessage(`Tip of the day: ${pickTip(void 0).text}`);
    context.subscriptions.push(
      nulis.commands.registerCommand("nulis.tips.show", async (argument) => {
        const tip = pickTip(requestedIndex(argument));
        await nulis.window.showInformationMessage(`Tip ${tip.index + 1} of ${TIPS.length}: ${tip.text}`);
        return { index: tip.index, text: tip.text };
      }),
      nulis.commands.registerCommand("nulis.tips.count", async () => {
        await nulis.window.showInformationMessage(`Nulis Tips knows ${TIPS.length} tips.`);
        return TIPS.length;
      })
    );
  };
  globalThis.activate = activate;
})();
