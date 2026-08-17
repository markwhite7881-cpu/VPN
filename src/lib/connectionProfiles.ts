export const LEGACY_SUBSCRIPTIONS_KEY = "singbox-client.subscriptions.v1";

export async function migrateLegacySubscriptions<T>(
  entries: unknown[],
  migrate: (entries: unknown[]) => Promise<T>,
  storage: Pick<Storage, "removeItem">,
): Promise<T | null> {
  if (!entries.length) return null;
  const result = await migrate(entries);
  storage.removeItem(LEGACY_SUBSCRIPTIONS_KEY);
  return result;
}
