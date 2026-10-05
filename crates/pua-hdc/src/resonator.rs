//! Resonator network for two factors (spec §4.5): given `q = a ⊗ b` with `a ∈ A`, `b ∈ B`,
//! recover `(a, b)` by alternating projections. No randomness; ties go to the lower id.

use core::fmt;

use crate::acc::{Accumulator, bundle};
use crate::codebook::{Codebook, Nearest};
use crate::hv::{Dim, Hv};

/// Hard ceiling on resonator iterations (spec §4.5).
pub const MAX_ITERATIONS: u8 = 16;

/// An iteration cap in `1..=16`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IterationCap(u8);

/// A cap outside `1..=16`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IterationCapError {
    /// The rejected value.
    pub value: u8,
}

impl fmt::Display for IterationCapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "resonator iteration cap {} outside 1..={MAX_ITERATIONS}",
            self.value
        )
    }
}

impl std::error::Error for IterationCapError {}

impl IterationCap {
    /// The spec's cap, 16.
    pub const MAX: Self = Self(MAX_ITERATIONS);

    /// A cap in `1..=16`.
    ///
    /// # Errors
    /// [`IterationCapError`] outside that range.
    pub const fn new(value: u8) -> Result<Self, IterationCapError> {
        if value >= 1 && value <= MAX_ITERATIONS {
            Ok(Self(value))
        } else {
            Err(IterationCapError { value })
        }
    }

    /// The cap.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl Default for IterationCap {
    fn default() -> Self {
        Self::MAX
    }
}

/// Outcome of [`resonate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Resonance {
    /// The estimates reached a fixed point.
    Converged {
        /// Best match for the first factor in `A`.
        a: Nearest,
        /// Best match for the second factor in `B`.
        b: Nearest,
        /// Iterations used (the last one confirmed the fixed point).
        iterations: u8,
    },
    /// No fixed point within the cap: the pack must abstain (`AbstainReason::NotConverged`).
    NotConverged {
        /// Iterations used (= the cap).
        iterations: u8,
    },
}

/// Projects `target` onto the span of `book`: `sign(Σ_i sim(target, v_i) · v_i)`.
fn project<D: Dim>(target: &Hv<D>, book: &Codebook<D>) -> Hv<D> {
    let mut acc = Accumulator::new();
    for v in book.vectors() {
        acc.add(v, i32::from(target.similarity(v).get()));
    }
    acc.to_hv()
}

/// Factorizes `q ≈ a ⊗ b` over codebooks `a_book` and `b_book`.
///
/// Starts from `â = bundle(A)`, `b̂ = bundle(B)`; each iteration updates
/// `â ← project(q ⊗ b̂, A)` then `b̂ ← project(q ⊗ â, B)`. Stops when neither estimate
/// changes, or after `cap` iterations.
pub fn resonate<D: Dim>(
    q: &Hv<D>,
    a_book: &Codebook<D>,
    b_book: &Codebook<D>,
    cap: IterationCap,
) -> Resonance {
    let mut a = bundle(a_book.vectors());
    let mut b = bundle(b_book.vectors());
    for it in 1..=cap.get() {
        let a2 = project(&q.bind(&b), a_book);
        let b2 = project(&q.bind(&a2), b_book);
        if a2 == a && b2 == b {
            return Resonance::Converged {
                a: a_book.nearest(&a),
                b: b_book.nearest(&b),
                iterations: it,
            };
        }
        a = a2;
        b = b2;
    }
    Resonance::NotConverged {
        iterations: cap.get(),
    }
}
