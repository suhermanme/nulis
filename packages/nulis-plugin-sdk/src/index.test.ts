import { describe, expect, test } from "vitest";
import { definePlugin, PLUGIN_API_VERSION } from "./index";

describe("plugin SDK", () => {
  test("declares an explicit API version", () => {
    expect(PLUGIN_API_VERSION).toBe(1);
  });

  test("defines a literal capability-scoped manifest", () => {
    const manifest = definePlugin({
      id: "nulis.example", name: "Example", version: "1.0.0",
      engines: { nulis: ">=0.1.0" }, main: "index.js",
      permissions: ["showMessage"],
    });
    expect(manifest.permissions).toEqual(["showMessage"]);
  });
});
