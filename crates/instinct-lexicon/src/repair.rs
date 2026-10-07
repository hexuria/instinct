//! Bounded edit distance for typo repair (spec §4.3).
//!
//! Distance is optimal string alignment (Damerau-Levenshtein without repeated edits of one
//! substring) over `char`s. Ties at equal distance are broken by a lexicographic cost vector:
//! an edit explained by a QWERTY-adjacent substitution beats a transposition, which beats any
//! other edit; remaining ties go to the lexically smaller term (applied by the caller). The
//! vector is summed along the alignment, and lexicographic order on `(edits, misses, others)`
//! is a total order compatible with addition, so the dynamic program finds the true minimum.

/// Cost of an alignment: `(edits, keyboard_misses, non_transpositions)`, compared
/// lexicographically.
pub(crate) type Cost = (u16, u16, u16);

const ZERO: Cost = (0, 0, 0);
const ADJACENT_SUB: Cost = (1, 0, 1);
const TRANSPOSE: Cost = (1, 1, 0);
const OTHER: Cost = (1, 1, 1);

fn add(a: Cost, b: Cost) -> Cost {
    (
        a.0.saturating_add(b.0),
        a.1.saturating_add(b.1),
        a.2.saturating_add(b.2),
    )
}

const ROWS: [&[u8]; 3] = [b"qwertyuiop", b"asdfghjkl", b"zxcvbnm"];

fn key_pos(c: char) -> Option<(usize, usize)> {
    let b = u8::try_from(c).ok()?;
    ROWS.iter()
        .enumerate()
        .find_map(|(r, row)| row.iter().position(|&k| k == b).map(|col| (r, col)))
}

/// Whether two lowercase ASCII letters are neighbours on a QWERTY keyboard (same row ±1, or the
/// two touching keys on the row above/below; rows are staggered right going down).
pub(crate) fn qwerty_adjacent(a: char, b: char) -> bool {
    let (Some((ra, ca)), Some((rb, cb))) = (key_pos(a), key_pos(b)) else {
        return false;
    };
    if ra == rb {
        return ca.abs_diff(cb) == 1;
    }
    // Row below is shifted right by half a key: key (r, c) touches (r+1, c-1) and (r+1, c).
    let ((r_hi, c_hi), (r_lo, c_lo)) = if ra < rb {
        ((ra, ca), (rb, cb))
    } else {
        ((rb, cb), (ra, ca))
    };
    r_lo == r_hi + 1 && (c_lo == c_hi || c_lo + 1 == c_hi)
}

/// Minimal alignment cost between `a` and `b`, or `None` when the edit count exceeds `max`.
#[allow(clippy::many_single_char_names)] // DP over (i, j) with the textbook names
pub(crate) fn cost(a: &[char], b: &[char], max: u16) -> Option<Cost> {
    if a.len().abs_diff(b.len()) > usize::from(max) {
        return None;
    }
    let w = b.len() + 1;
    let mut d = vec![ZERO; (a.len() + 1) * w];
    let at = |i: usize, j: usize| i * w + j;
    for i in 0..=a.len() {
        d[at(i, 0)] = mul(OTHER, i);
    }
    for j in 0..=b.len() {
        d[at(0, j)] = mul(OTHER, j);
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let (x, y) = (a[i - 1], b[j - 1]);
            let sub = if x == y {
                ZERO
            } else if qwerty_adjacent(x, y) {
                ADJACENT_SUB
            } else {
                OTHER
            };
            let mut best = add(d[at(i - 1, j - 1)], sub);
            best = best.min(add(d[at(i - 1, j)], OTHER));
            best = best.min(add(d[at(i, j - 1)], OTHER));
            if i > 1 && j > 1 && x == b[j - 2] && a[i - 2] == y && x != y {
                best = best.min(add(d[at(i - 2, j - 2)], TRANSPOSE));
            }
            d[at(i, j)] = best;
        }
    }
    let c = d[at(a.len(), b.len())];
    (c.0 <= max).then_some(c)
}

fn mul(c: Cost, n: usize) -> Cost {
    let n = u16::try_from(n).unwrap_or(u16::MAX);
    (
        c.0.saturating_mul(n),
        c.1.saturating_mul(n),
        c.2.saturating_mul(n),
    )
}

/// All char sequences obtained from `s` by deleting at most `max` chars (including `s`
/// itself), sorted and deduplicated. `s.len()` is bounded by the caller. Char vectors keep
/// generation allocation-light — callers collect to `String` only where an actual string key
/// is needed.
pub(crate) fn deletes(s: &[char], max: usize) -> Vec<Vec<char>> {
    let mut out = vec![s.to_vec()];
    let mut frontier = vec![s.to_vec()];
    for _ in 0..max {
        let mut next = Vec::new();
        for w in &frontier {
            for k in 0..w.len() {
                let mut v = w.clone();
                v.remove(k);
                next.push(v);
            }
        }
        next.sort_unstable();
        next.dedup();
        out.extend(next.iter().cloned());
        frontier = next;
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn adjacency_examples() {
        assert!(qwerty_adjacent('q', 'w'));
        assert!(qwerty_adjacent('q', 'a'));
        assert!(qwerty_adjacent('w', 'a'));
        assert!(qwerty_adjacent('s', 'z'));
        assert!(qwerty_adjacent('s', 'x'));
        assert!(qwerty_adjacent('p', 'l'));
        assert!(!qwerty_adjacent('q', 's'));
        assert!(qwerty_adjacent('a', 'z') && qwerty_adjacent('z', 'a'));
        assert!(!qwerty_adjacent('a', 'x'));
        assert!(!qwerty_adjacent('q', 'z'));
        assert!(!qwerty_adjacent('q', 'q'));
        assert!(!qwerty_adjacent('q', 'e'));
        assert!(!qwerty_adjacent('Q', 'W'));
        assert!(!qwerty_adjacent('ñ', 'n'));
    }

    #[test]
    fn costs() {
        assert_eq!(cost(&c("stop"), &c("stop"), 2), Some(ZERO));
        assert_eq!(cost(&c("stip"), &c("stop"), 2), Some(ADJACENT_SUB));
        assert_eq!(cost(&c("stxp"), &c("stop"), 2), Some(OTHER));
        assert_eq!(cost(&c("tsop"), &c("stop"), 2), Some(TRANSPOSE));
        assert_eq!(cost(&c("sop"), &c("stop"), 2), Some(OTHER));
        assert_eq!(cost(&c("sstop"), &c("stop"), 2), Some(OTHER));
        assert_eq!(cost(&c("xxxx"), &c("stop"), 2), None);
        assert_eq!(cost(&c("st"), &c("stop"), 1), None);
        assert_eq!(cost(&c("st"), &c("stop"), 2), Some(mul(OTHER, 2)));
        assert_eq!(cost(&c(""), &c(""), 0), Some(ZERO));
    }

    fn ss(v: Vec<Vec<char>>) -> Vec<String> {
        v.into_iter().map(|w| w.into_iter().collect()).collect()
    }

    #[test]
    fn deletes_enumerates_and_dedups() {
        assert_eq!(ss(deletes(&c("ab"), 1)), ["a", "ab", "b"]);
        assert_eq!(ss(deletes(&c("aa"), 2)), ["", "a", "aa"]);
        assert_eq!(ss(deletes(&c("abc"), 0)), ["abc"]);
    }
}
