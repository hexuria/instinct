//! Integer bundling (spec §4.5): a sum, then a sign.

use crate::hv::{Dim, Hv};

/// Per-component `i32` sums of weighted `±1` vectors. Thresholding maps `> 0` to `+1`,
/// `< 0` to `−1` and the tie `0` to `+1`. Decoding reads the sums, not the thresholded vector.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Accumulator<D: Dim> {
    sums: Box<[i32]>,
    _d: core::marker::PhantomData<D>,
}

impl<D: Dim> Default for Accumulator<D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<D: Dim> Accumulator<D> {
    /// All zeros.
    pub fn new() -> Self {
        Self {
            sums: vec![0; D::BITS as usize].into_boxed_slice(),
            _d: core::marker::PhantomData,
        }
    }

    /// Adds `weight · v` (saturating per component).
    pub fn add(&mut self, v: &Hv<D>, weight: i32) {
        let neg = weight.saturating_neg();
        for (k, word) in v.words().iter().enumerate() {
            for b in 0..64 {
                let s = &mut self.sums[k * 64 + b];
                *s = s.saturating_add(if (word >> b) & 1 == 0 { weight } else { neg });
            }
        }
    }

    /// The component sums.
    pub fn sums(&self) -> &[i32] {
        &self.sums
    }

    /// `Σ sums_i · v_i`.
    pub fn dot(&self, v: &Hv<D>) -> i64 {
        let mut total = 0i64;
        for (k, word) in v.words().iter().enumerate() {
            for b in 0..64 {
                let s = i64::from(self.sums[k * 64 + b]);
                if (word >> b) & 1 == 0 {
                    total += s;
                } else {
                    total -= s;
                }
            }
        }
        total
    }

    /// `Σ |sums_i|`, the largest `|dot|` any vector can reach.
    pub fn l1(&self) -> i64 {
        self.sums.iter().map(|s| i64::from(s.unsigned_abs())).sum()
    }

    /// Sign with the zero → `+1` tie.
    pub fn to_hv(&self) -> Hv<D> {
        let mut w = D::zero_words();
        for (k, out) in w.as_mut().iter_mut().enumerate() {
            let mut x = 0u64;
            for b in 0..64 {
                if self.sums[k * 64 + b] < 0 {
                    x |= 1 << b;
                }
            }
            *out = x;
        }
        Hv::from_words(w)
    }
}

/// Unweighted bundle (majority with the zero → `+1` tie). Invariant under input order.
pub fn bundle<D: Dim>(items: &[Hv<D>]) -> Hv<D> {
    let mut acc = Accumulator::new();
    for v in items {
        acc.add(v, 1);
    }
    acc.to_hv()
}
