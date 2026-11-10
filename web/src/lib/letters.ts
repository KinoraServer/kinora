/**
 * The A–Z jump rail on library pages. Titles are grouped the way the server
 * sorts them (`sort_title` in crates/server/src/scanner.rs): a leading "The",
 * "A" or "An" is ignored, so "The Matrix" is under M. Anything that doesn't
 * start with a letter goes under "#".
 */

export const OTHER = "#";
export const LETTERS: readonly string[] = [OTHER, ..."ABCDEFGHIJKLMNOPQRSTUVWXYZ"];

const ARTICLES = ["the ", "a ", "an "];

/** The rail letter a title belongs under. */
export function letterOf(title: string): string {
  let t = title.trim().toLowerCase();
  for (const article of ARTICLES) {
    if (t.startsWith(article)) {
      t = t.slice(article.length);
      break;
    }
  }
  // "Élan" → E, "Ødegaard" stays "#" (no plain letter to fall back to).
  const first = t.normalize("NFD").charAt(0).toUpperCase();
  return first >= "A" && first <= "Z" ? first : OTHER;
}

/** For each letter that has titles, the index of its first one in `titles`. */
export function firstIndexByLetter(titles: readonly string[]): Map<string, number> {
  const out = new Map<string, number>();
  titles.forEach((title, i) => {
    const letter = letterOf(title);
    if (!out.has(letter)) out.set(letter, i);
  });
  return out;
}

/**
 * The index of the first element whose bottom edge is below `y`, assuming the
 * elements are in reading order (a grid, row by row). Binary search, so a
 * scroll handler only measures a dozen tiles even in a library of thousands.
 */
export function firstBelow(count: number, bottomOf: (index: number) => number, y: number): number {
  let lo = 0;
  let hi = count - 1;
  let found = count - 1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (bottomOf(mid) > y) {
      found = mid;
      hi = mid - 1;
    } else {
      lo = mid + 1;
    }
  }
  return found;
}
