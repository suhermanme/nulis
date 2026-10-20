import { expect, test } from "vitest";
import coreSource from "../../../crates/nulis-core/src/lib.rs?raw";
import { IPC_PROTOCOL_VERSION } from "./contracts";

test("the desktop protocol version matches nulis-core's", () => {
  const version = /pub const IPC_PROTOCOL_VERSION: u32 = (\d+);/.exec(coreSource)?.[1];
  expect(Number(version)).toBe(IPC_PROTOCOL_VERSION);
});
