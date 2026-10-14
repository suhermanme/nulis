globalThis.activate = async (nulis, context) => {
  context.subscriptions.push(
    nulis.commands.registerCommand("nulis.hello-example.hello", async () => {
      await nulis.window.showInformationMessage(
        "Hello from an independently built Nulis plugin.",
      );
      return null;
    }),
  );
};
