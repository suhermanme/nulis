import { configure } from "@testing-library/react";


// A jsdom window under a newer Node can come up without `window.localStorage`.
// Tests that save settings then read them back fail on the missing object. A
// Map-backed stand-in gives every suite the storage API it expects.
if (typeof window !== "undefined" && typeof window.localStorage === "undefined") {
  const store = new Map<string, string>();
  const storage: Storage = {
    clear: () => store.clear(),
    getItem: (key: string) => (store.has(key) ? store.get(key)! : null),
    key: (index: number) => [...store.keys()][index] ?? null,
    removeItem: (key: string) => void store.delete(key),
    setItem: (key: string, value: string) => void store.set(key, String(value)),
    get length() {
      return store.size;
    },
  };
  Object.defineProperty(window, "localStorage", { value: storage, writable: true });
  Object.defineProperty(globalThis, "localStorage", { value: storage, writable: true });
}
// Shared CI machines are several times slower than a developer's. The default
// one-second wait for asynchronous updates is too tight there and makes
// otherwise correct tests fail at random, so waits get more room. A passing
// wait still returns as soon as its condition holds.
configure({ asyncUtilTimeout: 5000 });
