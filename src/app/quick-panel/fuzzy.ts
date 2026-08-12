// [GRAIN] Quick Panel fuzzy matcher — fzf-lite subsequence scoring, no model.
//
// Deliberately keyword/alias driven (see registry.ts). For a bounded, known list
// of ~50 settings this beats an embedding model on precision, latency and RAM,
// which is exactly what a command palette needs: instant, offline, zero resident
// weight. Semantic re-ranking (the shared BGE engine) can layer on top later,
// but only earns its 130 MB download over a large, open corpus — not this one.

const WORD_BOUNDARY = /[\s\-_./]/;

/**
 * Score one query against one candidate string. Higher is better.
 * Returns `null` when the query is not an ordered subsequence of the text.
 */
export function scoreText(query: string, text: string): number | null {
  const q = query.toLowerCase();
  const t = text.toLowerCase();
  if (q.length === 0) return 0;

  // Contiguous substring is the strongest signal. Reward an earlier position,
  // a word-aligned start, and a fuller match relative to the candidate length.
  const idx = t.indexOf(q);
  if (idx >= 0) {
    let score = 1000 - idx * 4;
    if (idx === 0 || WORD_BOUNDARY.test(t[idx - 1] ?? "")) score += 120;
    score += Math.round((q.length / t.length) * 100);
    return score;
  }

  // Ordered subsequence: every query char appears, in order. Reward contiguous
  // runs and matches that land on a word start ("mtwr" → "Mute while recording").
  let ti = 0;
  let qi = 0;
  let score = 0;
  let prev = -2;
  while (qi < q.length && ti < t.length) {
    if (t[ti] === q[qi]) {
      let inc = 8;
      if (ti === prev + 1) inc += 10;
      if (ti === 0 || WORD_BOUNDARY.test(t[ti - 1] ?? "")) inc += 12;
      score += inc;
      prev = ti;
      qi += 1;
    }
    ti += 1;
  }
  if (qi < q.length) return null; // not every query char matched
  score -= text.length - q.length; // prefer tighter candidates
  return score;
}
