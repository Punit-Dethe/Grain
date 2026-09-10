// Rust adaptation of FluidAudio AsrChunkTokenMerger (Apache-2.0).
// See ../NOTICE. In-place accumulation is a Grain change.

/// One decoded token. Keep model-native IDs until the entire result is merged;
/// v3 byte-fallback tokens must be detokenized as a sequence, never individually.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Token {
    pub id: i32,
    pub frame: u64,
    pub confidence: f32,
    pub duration: u32,
}

/// Append an adjacent window using Fluid's overlap policy. The accumulator is
/// deliberately not sorted: Fluid folds windows in this order, then stable-sorts
/// only the final/preview view by timestamp. Sorting here changes later merges.
pub fn merge_tokens(left: &mut Vec<Token>, right: &[Token]) {
    let (Some(last), Some(first)) = (left.last(), right.first()) else {
        left.extend_from_slice(right);
        return;
    };
    let left_end = start_time(*last) + FRAME_SECONDS;
    let right_start = start_time(*first);
    if left_end <= right_start {
        left.extend_from_slice(right);
        return;
    }

    // Preserve Swift's Double operation order, including rounding at equality
    // edges. Integer frame cuts look equivalent but can select different tokens.
    let a: Vec<usize> = left
        .iter()
        .enumerate()
        .filter(|(_, t)| start_time(**t) + FRAME_SECONDS > right_start - 2.0)
        .map(|(i, _)| i)
        .collect();
    let b: Vec<usize> = right
        .iter()
        .enumerate()
        .filter(|(_, t)| start_time(**t) < left_end + 2.0)
        .map(|(i, _)| i)
        .collect();
    if a.len() >= 2 && b.len() >= 2 {
        let mut pairs = contiguous(left, right, &a, &b);
        if pairs.len() < (a.len() / 2).max(1) {
            pairs = lcs(left, right, &a, &b);
        }
        if !pairs.is_empty() {
            splice(left, right, &a, &b, &pairs);
            return;
        }
    }

    // Fluid includes both sides exactly on the midpoint; preserve this even
    // when it produces two tokens at one timestamp.
    let cutoff = (left_end + right_start) / 2.0;
    left.retain(|t| start_time(*t) <= cutoff);
    left.extend(right.iter().filter(|t| start_time(**t) >= cutoff).copied());
}

const FRAME_SECONDS: f64 = 1_280.0 / 16_000.0;

fn start_time(token: Token) -> f64 {
    token.frame as f64 * FRAME_SECONDS
}

fn matches(left: Token, right: Token) -> bool {
    left.id == right.id && (start_time(left) - start_time(right)).abs() < 1.0
}

fn contiguous(left: &[Token], right: &[Token], a: &[usize], b: &[usize]) -> Vec<(usize, usize)> {
    let mut best = (0, 0, 0);
    for i in 0..a.len() {
        for j in 0..b.len() {
            let mut count = 0;
            while i + count < a.len()
                && j + count < b.len()
                && matches(left[a[i + count]], right[b[j + count]])
            {
                count += 1;
            }
            // Strict comparison preserves Fluid's first-match tie break.
            if count > best.2 {
                best = (i, j, count);
            }
        }
    }
    (0..best.2).map(|n| (best.0 + n, best.1 + n)).collect()
}

fn lcs(left: &[Token], right: &[Token], a: &[usize], b: &[usize]) -> Vec<(usize, usize)> {
    let width = b.len() + 1;
    // Only overlap tokens enter this table, never the entire transcript.
    let mut dp = vec![0_usize; (a.len() + 1) * width];
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            dp[i * width + j] = if matches(left[a[i - 1]], right[b[j - 1]]) {
                dp[(i - 1) * width + j - 1] + 1
            } else {
                dp[(i - 1) * width + j].max(dp[i * width + j - 1])
            };
        }
    }
    let (mut i, mut j) = (a.len(), b.len());
    let mut pairs = Vec::new();
    while i > 0 && j > 0 {
        if matches(left[a[i - 1]], right[b[j - 1]]) {
            pairs.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if dp[(i - 1) * width + j] > dp[i * width + j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    pairs.reverse();
    pairs
}

fn splice(
    left: &mut Vec<Token>,
    right: &[Token],
    a: &[usize],
    b: &[usize],
    pairs: &[(usize, usize)],
) {
    // Rebuild just the overlap suffix. Retain the long stable transcript prefix
    // in its original allocation instead of cloning it on every stable window.
    let first = a[pairs[0].0];
    let mut suffix = Vec::new();
    for (index, &(i, j)) in pairs.iter().enumerate() {
        let (li, ri) = (a[i], b[j]);
        suffix.push(left[li]); // matched tokens keep the left confidence/duration
        if let Some(&(next_i, next_j)) = pairs.get(index + 1) {
            let gap_left = &left[li + 1..a[next_i]];
            let gap_right = &right[ri + 1..b[next_j]];
            suffix.extend_from_slice(if gap_right.len() > gap_left.len() {
                gap_right
            } else {
                gap_left
            });
        }
    }
    let last_right = b[pairs.last().unwrap().1];
    suffix.extend_from_slice(&right[last_right + 1..]);
    left.truncate(first);
    left.extend(suffix);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn token(id: i32, frame: u64) -> Token {
        Token {
            id,
            frame,
            confidence: 0.75,
            duration: 2,
        }
    }
    fn ids(tokens: &[Token]) -> Vec<i32> {
        tokens.iter().map(|t| t.id).collect()
    }

    #[test]
    fn empty_and_time_disjoint_windows_preserve_every_token() {
        let mut left = Vec::new();
        merge_tokens(&mut left, &[token(1, 0)]);
        merge_tokens(&mut left, &[]);
        merge_tokens(&mut left, &[token(1, 1), token(2, 2)]);
        assert_eq!(ids(&left), [1, 1, 2]);
    }

    #[test]
    fn contiguous_overlap_keeps_native_left_metadata() {
        let mut left = vec![token(10, 80), token(11, 90), token(12, 100)];
        let mut right = vec![token(11, 91), token(12, 101), token(13, 110)];
        right[0].confidence = 0.99;
        right[0].duration = 4;
        merge_tokens(&mut left, &right);
        assert_eq!(
            left,
            [token(10, 80), token(11, 90), token(12, 100), token(13, 110)]
        );
    }

    #[test]
    fn lcs_keeps_longer_internal_gap_and_left_on_equal_gap() {
        let mut left = vec![
            token(1, 80),
            token(7, 82),
            token(2, 84),
            token(8, 86),
            token(3, 88),
            token(9, 90),
        ];
        let right = [
            token(1, 80),
            token(20, 81),
            token(21, 82),
            token(2, 84),
            token(22, 86),
            token(3, 88),
            token(30, 95),
        ];
        merge_tokens(&mut left, &right);
        assert_eq!(ids(&left), [1, 20, 21, 2, 8, 3, 30]);
    }

    #[test]
    fn unmatched_overlap_uses_inclusive_midpoint() {
        let mut left = vec![token(1, 8), token(2, 10), token(3, 11)];
        let right = [token(4, 8), token(5, 10), token(6, 12)];
        merge_tokens(&mut left, &right); // cutoff (11+1+8)/2 = 10
        assert_eq!(ids(&left), [1, 2, 5, 6]);
    }

    #[test]
    fn matching_tolerance_is_strictly_less_than_one_second() {
        assert!(matches(token(1, 100), token(1, 112)));
        assert!(!matches(token(1, 100), token(1, 113)));
        assert!(!matches(token(1, 100), token(2, 100)));
    }

    #[test]
    fn distant_repeated_phrase_is_not_mistaken_for_overlap() {
        let mut left = vec![token(1, 0), token(2, 1), token(1, 100), token(2, 101)];
        merge_tokens(&mut left, &[token(1, 100), token(2, 101), token(3, 105)]);
        assert_eq!(ids(&left), [1, 2, 1, 2, 3]);
    }

    #[test]
    fn contiguous_ties_choose_the_first_match() {
        let mut left = vec![token(1, 10), token(2, 11), token(1, 12), token(2, 13)];
        merge_tokens(&mut left, &[token(1, 10), token(2, 11), token(3, 14)]);
        assert_eq!(ids(&left), [1, 2, 3]);
    }

    #[test]
    fn lcs_ties_follow_fluid_backtracking_direction() {
        let left = [token(1, 10), token(2, 11), token(3, 12), token(4, 13)];
        let right = [token(2, 10), token(1, 11), token(4, 12), token(3, 13)];
        assert_eq!(
            lcs(&left, &right, &[0, 1, 2, 3], &[0, 1, 2, 3]),
            [(1, 0), (3, 2)]
        );
    }

    #[test]
    fn output_view_sort_must_not_reorder_accumulator() {
        let mut left = vec![token(1, 10), token(2, 12), token(3, 14), token(4, 16)];
        merge_tokens(
            &mut left,
            &[
                token(1, 10),
                token(8, 10),
                token(9, 11),
                token(3, 12),
                token(5, 13),
            ],
        );
        assert_eq!(ids(&left), [1, 8, 9, 3, 5]);
        assert_eq!(
            left.iter().map(|t| t.frame).collect::<Vec<_>>(),
            [10, 10, 11, 14, 13]
        );
        let mut view = left.clone();
        view.sort_by_key(|t| t.frame);
        assert_eq!(ids(&view), [1, 8, 9, 5, 3]);
        assert_eq!(ids(&left), [1, 8, 9, 3, 5]);
    }

    #[test]
    fn midpoint_preserves_reference_floating_equality_behavior() {
        let mut left = vec![token(1, 1), token(2, 3), token(3, 4)];
        let right = [token(4, 1), token(5, 3), token(6, 6)];
        merge_tokens(&mut left, &right);
        // Swift's cutoff is 0.24000000000000002, right frame 3 is 0.24.
        // An integer implementation incorrectly retains id 5 at this boundary.
        assert_eq!(ids(&left), [1, 2, 6]);
    }
}
