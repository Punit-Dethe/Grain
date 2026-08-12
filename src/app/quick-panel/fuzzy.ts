// [GRAIN] Quick Panel matcher — model-free ranking for a bounded, known list.
//
// For ~50 settings a curated keyword index out-ranks an embedding model on
// precision, latency and RAM, so the intelligence lives here in the scorer, not
// in vectors. Three ideas, all borrowed from the fuzzy-finder literature:
//
//   1. fzf-style scoring — word-boundary/prefix bonuses with a gap penalty, so
//      a tight "mtwr" → "Mute while recording" beats an incidental sprawl.
//   2. token AND matching — the query is split on spaces and every token must
//      land somewhere, order-independent, so "recording mute" and "dark theme"
//      both work across a title + its aliases.
//   3. Damerau–Levenshtein typo tolerance — "recroding" still finds Recording,
//      but always ranked below a clean match.
//
// Refs: fzf FuzzyMatchV2 (Smith–Waterman scoring), Algolia/Typesense typo model.

const BOUNDARY = /[\s\-_./]/;

const isBoundary = (ch: string | undefined): boolean =>
  ch === undefined || BOUNDARY.test(ch);

/**
 * Score one lower-cased token against one text. Higher is better; `null` when
 * the token is not an ordered subsequence of the text.
 */
function scoreToken(token: string, text: string): number | null {
  const t = text.toLowerCase();

  // Contiguous substring — the strongest signal. Reward an earlier, word-aligned,
  // fuller match.
  const idx = t.indexOf(token);
  if (idx >= 0) {
    let score = 100 - idx * 2;
    if (isBoundary(t[idx - 1])) score += 40;
    score += Math.round((token.length / t.length) * 30);
    return score;
  }

  // Ordered subsequence with fzf-style bonuses. A word-boundary hit is worth a
  // few characters of gap but no more, which keeps this a fuzzy finder rather
  // than a pure acronym matcher.
  let ti = 0;
  let qi = 0;
  let score = 0;
  let prev = -1;
  let first = -1;
  while (qi < token.length && ti < t.length) {
    if (t[ti] === token[qi]) {
      if (first < 0) first = ti;
      let inc = 6;
      if (prev >= 0) {
        const gap = ti - prev - 1;
        inc += gap === 0 ? 8 : -Math.min(gap, 6);
      }
      if (isBoundary(t[ti - 1])) inc += 10;
      score += inc;
      prev = ti;
      qi += 1;
    }
    ti += 1;
  }
  if (qi < token.length) return null;
  return score - first * 0.5;
}

/** Bounded Damerau–Levenshtein distance; returns `max + 1` once it is exceeded. */
function boundedDamerau(a: string, b: string, max: number): number {
  const al = a.length;
  const bl = b.length;
  if (Math.abs(al - bl) > max) return max + 1;

  let prevPrev = new Array<number>(bl + 1).fill(0);
  let prev = new Array<number>(bl + 1);
  let curr = new Array<number>(bl + 1);
  for (let j = 0; j <= bl; j += 1) prev[j] = j;

  for (let i = 1; i <= al; i += 1) {
    curr[0] = i;
    let rowMin = curr[0];
    for (let j = 1; j <= bl; j += 1) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      let value = Math.min(
        prev[j] + 1, // deletion
        curr[j - 1] + 1, // insertion
        prev[j - 1] + cost, // substitution
      );
      if (i > 1 && j > 1 && a[i - 1] === b[j - 2] && a[i - 2] === b[j - 1]) {
        value = Math.min(value, prevPrev[j - 2] + 1); // transposition
      }
      curr[j] = value;
      if (value < rowMin) rowMin = value;
    }
    if (rowMin > max) return max + 1; // no path can recover — bail early
    [prevPrev, prev, curr] = [prev, curr, prevPrev];
  }
  return prev[bl];
}

/** Typo-tolerant score for a token against any single word of the text. */
function typoScore(token: string, text: string): number | null {
  if (token.length < 4) return null; // short tokens stay exact
  const max = token.length >= 7 ? 2 : 1;
  let best = max + 1;
  for (const word of text.toLowerCase().split(BOUNDARY)) {
    if (!word || Math.abs(word.length - token.length) > max) continue;
    const dist = boundedDamerau(token, word, max);
    if (dist < best) best = dist;
    if (best === 1) break;
  }
  if (best > max) return null;
  return 30 - best * 12; // always below a clean subsequence match
}

/**
 * Rank a query against an item's title and aliases. Every whitespace token must
 * match some field (order-independent); the item score is the sum of each
 * token's best field score. Returns `null` when any token matches nothing.
 */
export function scoreItem(
  query: string,
  title: string,
  keywords: string[],
): number | null {
  const trimmed = query.trim().toLowerCase();
  if (trimmed.length === 0) return 0;
  const tokens = trimmed.split(/\s+/);

  let total = 0;
  for (const token of tokens) {
    let best = scoreToken(token, title);
    for (const keyword of keywords) {
      const s = scoreToken(token, keyword);
      if (s !== null)
        best = best === null ? s * 0.75 : Math.max(best, s * 0.75);
    }
    if (best === null) {
      // Nothing matched cleanly — allow a typo, weighting title over aliases.
      let typo = typoScore(token, title);
      for (const keyword of keywords) {
        const s = typoScore(token, keyword);
        if (s !== null)
          typo = typo === null ? s * 0.75 : Math.max(typo, s * 0.75);
      }
      best = typo;
    }
    if (best === null) return null;
    total += best;
  }

  // A contiguous multi-word hit in the title is a strong intent signal.
  if (tokens.length > 1 && title.toLowerCase().includes(trimmed)) total += 40;
  return total;
}
