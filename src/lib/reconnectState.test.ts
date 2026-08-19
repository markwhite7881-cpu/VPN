import { describe, expect, it } from "vitest";
import { nextReconnectRequired, shouldShowReconnectNotice } from "./reconnectState";

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

describe("shouldShowReconnectNotice", () => {
  it("keeps one notice visible after multiple generic edits while running", () => {
    const pendingAfterFirstEdit = nextReconnectRequired(false, true);
    const pendingAfterSecondEdit = nextReconnectRequired(pendingAfterFirstEdit, true);

    expect(shouldShowReconnectNotice({
      reconnectInProgress: false,
      reconnectRequired: pendingAfterSecondEdit,
      reconnectFailed: false,
      status: "running",
    })).toBe(true);
  });

  it("keeps the notice hidden during initial hydration and tab-only navigation", () => {
    expect(shouldShowReconnectNotice({
      reconnectInProgress: false,
      reconnectRequired: false,
      reconnectFailed: false,
      status: "running",
    })).toBe(false);
  });

  it("hides a resolved running notice after a successful reconnect", () => {
    expect(shouldShowReconnectNotice({
      reconnectInProgress: false,
      reconnectRequired: false,
      reconnectFailed: false,
      status: "running",
    })).toBe(false);
  });

  it("keeps retry visible after a failed reconnect stops the server", () => {
    expect(shouldShowReconnectNotice({
      reconnectInProgress: false,
      reconnectRequired: true,
      reconnectFailed: true,
      status: "stopped",
    })).toBe(true);
  });

  it("keeps ordinary stopped-state edits silent", () => {
    expect(shouldShowReconnectNotice({
      reconnectInProgress: false,
      reconnectRequired: nextReconnectRequired(false, false),
      reconnectFailed: false,
      status: "stopped",
    })).toBe(false);
  });

  it("keeps the notice visible while reconnect work is in progress", () => {
    expect(shouldShowReconnectNotice({
      reconnectInProgress: true,
      reconnectRequired: false,
      reconnectFailed: false,
      status: "stopping",
    })).toBe(true);
  });
});
