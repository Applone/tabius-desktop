export function reorderTabs<T extends { id: string }>(
  items: T[],
  source: string,
  target: string,
  after: boolean,
): T[] {
  if (
    source === target ||
    !items.some((item) => item.id === source) ||
    !items.some((item) => item.id === target)
  )
    return items;
  const next = items.filter((item) => item.id !== source);
  const index = next.findIndex((item) => item.id === target) + Number(after);
  const moved = items.find((item) => item.id === source);
  if (moved) next.splice(index, 0, moved);
  return next;
}
export function adjacentTab(items: { id: string }[], closing: string): string {
  const index = items.findIndex((item) => item.id === closing);
  return (items[index + 1] ?? items[index - 1])?.id ?? "library";
}
export function cycleTab(
  items: { id: string }[],
  active: string,
  direction: number,
): string {
  const ids = ["library", "sftp", ...items.map((item) => item.id)];
  const index = Math.max(0, ids.indexOf(active));
  return ids[(index + direction + ids.length) % ids.length];
}
