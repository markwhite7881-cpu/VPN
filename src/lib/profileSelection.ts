export function isValidProfileSelection(index: number, profileCount: number): boolean {
  return index === -1 || (index >= 0 && index < profileCount);
}
