import { describe, expect, it, vi } from "vitest";
import { LEGACY_SUBSCRIPTIONS_KEY, migrateLegacySubscriptions } from "./connectionProfiles";

describe("legacy subscription migration", () => {
  it("keeps storage when backend migration fails", async () => {
    const storage = { removeItem: vi.fn() } as unknown as Storage;
    await expect(migrateLegacySubscriptions(["legacy"], async () => { throw new Error("failed"); }, storage)).rejects.toThrow("failed");
    expect(storage.removeItem).not.toHaveBeenCalled();
  });

  it("removes storage only after backend migration succeeds", async () => {
    const storage = { removeItem: vi.fn() } as unknown as Storage;
    await expect(migrateLegacySubscriptions(["legacy"], async () => "ok", storage)).resolves.toBe("ok");
    expect(storage.removeItem).toHaveBeenCalledWith(LEGACY_SUBSCRIPTIONS_KEY);
  });
});
