import { describe, expect, it } from "vitest";
import { nextReconnectRequired } from "./reconnectState";

describe("nextReconnectRequired", () => {
  it("does not mark stopped settings edits as pending", () => {
    expect(nextReconnectRequired(false, false)).toBe(false);
  });

  it("does not clear an existing pending flag while stopped", () => {
    expect(nextReconnectRequired(true, false)).toBe(true);
  });

  it("marks the first running settings edit as pending", () => {
    expect(nextReconnectRequired(false, true)).toBe(true);
  });

  it("keeps running settings edits pending", () => {
    expect(nextReconnectRequired(true, true)).toBe(true);
  });
});
