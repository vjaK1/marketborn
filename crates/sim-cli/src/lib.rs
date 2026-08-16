//! Library half of sim-cli: pure helpers, unit-testable without a binary,
//! plus the websocket `serve` transport.

pub mod serve;

/// First tick present in both manifests whose hashes differ. `None` means
/// the common range agrees everywhere.
pub fn first_divergence(a: &[(u64, String)], b: &[(u64, String)]) -> Option<u64> {
    let mut ai = a.iter().peekable();
    let mut bi = b.iter().peekable();
    while let (Some((ta, ha)), Some((tb, hb))) = (ai.peek(), bi.peek()) {
        match ta.cmp(tb) {
            std::cmp::Ordering::Less => {
                ai.next();
            }
            std::cmp::Ordering::Greater => {
                bi.next();
            }
            std::cmp::Ordering::Equal => {
                if ha != hb {
                    return Some(*ta);
                }
                ai.next();
                bi.next();
            }
        }
    }
    None
}

/// Last tick both manifests share (for "identical through tick N" output).
pub fn last_common_tick(a: &[(u64, String)], b: &[(u64, String)]) -> Option<u64> {
    let b_ticks: std::collections::BTreeSet<u64> = b.iter().map(|(t, _)| *t).collect();
    a.iter()
        .map(|(t, _)| *t)
        .filter(|t| b_ticks.contains(t))
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(entries: &[(u64, &str)]) -> Vec<(u64, String)> {
        entries.iter().map(|(t, h)| (*t, h.to_string())).collect()
    }

    #[test]
    fn identical_manifests_have_no_divergence() {
        let a = m(&[(0, "x"), (50, "y"), (100, "z")]);
        assert_eq!(first_divergence(&a, &a), None);
        assert_eq!(last_common_tick(&a, &a), Some(100));
    }

    #[test]
    fn divergence_reports_first_differing_common_tick() {
        let a = m(&[(0, "x"), (50, "y"), (100, "z")]);
        let b = m(&[(0, "x"), (50, "y"), (100, "w"), (150, "v")]);
        assert_eq!(first_divergence(&a, &b), Some(100));
    }

    #[test]
    fn misaligned_ticks_are_skipped_not_compared() {
        let a = m(&[(0, "x"), (50, "y")]);
        let b = m(&[(0, "x"), (75, "q"), (150, "v")]);
        assert_eq!(first_divergence(&a, &b), None);
        assert_eq!(last_common_tick(&a, &b), Some(0));
    }
}
