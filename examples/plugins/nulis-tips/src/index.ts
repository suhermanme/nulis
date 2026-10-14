import type { Activate, JsonValue } from "nulis-plugin-sdk";
import { pickTip, TIPS } from "./tips";

/** Reads `{ "index": 3 }` from a command's argument; the palette passes `{}`. */
function requestedIndex(argument: JsonValue | undefined): unknown {
  return argument !== null && typeof argument === "object" && !Array.isArray(argument)
    ? argument.index
    : undefined;
}

const activate: Activate = (nulis, context) => {
  // `activate` runs at startup (the "onStartup" event) and again before every
  // command. Only the last message of a run is shown, so at startup this tip
  // of the day appears, and when a command runs its own message replaces it.
  void nulis.window.showInformationMessage(`Tip of the day: ${pickTip(undefined).text}`);

  // Every command in plugin.json must be registered here, on every activation.
  context.subscriptions.push(
    nulis.commands.registerCommand("nulis.tips.show", async (argument) => {
      const tip = pickTip(requestedIndex(argument));
      await nulis.window.showInformationMessage(`Tip ${tip.index + 1} of ${TIPS.length}: ${tip.text}`);
      // The result goes back to whoever ran the command.
      return { index: tip.index, text: tip.text };
    }),
    nulis.commands.registerCommand("nulis.tips.count", async () => {
      await nulis.window.showInformationMessage(`Nulis Tips knows ${TIPS.length} tips.`);
      return TIPS.length;
    }),
  );
};

(globalThis as typeof globalThis & { activate?: Activate }).activate = activate;
