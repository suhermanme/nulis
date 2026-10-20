import { expect, test } from "vitest";
import { matchScore, rankMatches } from "./quickOpen";

test("file name matches rank above path matches and scattered letters", () => {
  const files = ["src/app/main.rs", "docs/main-notes.md", "src/domain.rs", "README.md"].map((path) => ({
    path, name: path.split("/").at(-1) ?? path,
  }));
  const ranked = (query: string) => rankMatches(files, query, (file) => file, 10).map((file) => file.path);
  expect(ranked("main")).toEqual(["src/app/main.rs", "docs/main-notes.md", "src/domain.rs"]);
  expect(ranked("app")).toEqual(["src/app/main.rs"]);
  expect(ranked("srm")).toEqual(["src/app/main.rs", "src/domain.rs"]);
  expect(ranked("zzz")).toEqual([]);
  expect(ranked("")).toHaveLength(4);
  expect(matchScore("README.md", "README.md", "README.md")).toBe(0);
});
