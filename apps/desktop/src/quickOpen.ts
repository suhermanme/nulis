/** Matching for Quick Open: a file name match beats a match spread over the path. */

/** Lower is better; `null` when `query` does not match `path` at all. */
export function matchScore(query: string, name: string, path: string): number | null {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return 0;
  const lowerName = name.toLocaleLowerCase();
  const lowerPath = path.toLocaleLowerCase();
  if (lowerName === needle) return 0;
  if (lowerName.startsWith(needle)) return 1;
  if (lowerName.includes(needle)) return 2;
  if (lowerPath.includes(needle)) return 3;
  // The letters in order, anywhere in the path; tighter runs rank higher.
  let from = 0;
  let gaps = 0;
  for (const character of needle) {
    const found = lowerPath.indexOf(character, from);
    if (found < 0) return null;
    if (found > from) gaps += 1;
    from = found + 1;
  }
  return 4 + gaps / (needle.length + 1);
}

/** The best `limit` matches, best first; ties keep their order. */
export function rankMatches<T>(items: readonly T[], query: string, key: (item: T) => { name: string; path: string }, limit: number): T[] {
  return items
    .map((item, order) => ({ item, order, score: matchScore(query, key(item).name, key(item).path) }))
    .filter((entry): entry is { item: T; order: number; score: number } => entry.score !== null)
    .sort((left, right) => left.score - right.score || left.order - right.order)
    .slice(0, limit)
    .map((entry) => entry.item);
}
