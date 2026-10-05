//! Dimensions and packed bipolar hypervectors.

use core::fmt;
use core::hash::Hash;
use core::marker::PhantomData;

use pua_core::Millis;

mod sealed {
    pub trait Sealed {}
}

/// A supported dimension. Sealed: only [`D1024`], [`D2048`] and [`D4096`] exist, and vectors of
/// different dimensions are different types, so mixing them is a compile error.
pub trait Dim: sealed::Sealed + Copy + Eq + Ord + Hash + fmt::Debug + Default + 'static {
    /// Number of bits (= components).
    const BITS: u32;
    /// Packed storage.
    type Words: Copy + Eq + Ord + Hash + fmt::Debug + AsRef<[u64]> + AsMut<[u64]>;
    /// All-zero storage (every component `+1`).
    fn zero_words() -> Self::Words;
}

macro_rules! dim {
    ($name:ident, $bits:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
        pub struct $name;
        impl sealed::Sealed for $name {}
        impl Dim for $name {
            const BITS: u32 = $bits;
            type Words = [u64; $bits / 64];
            fn zero_words() -> Self::Words {
                [0; $bits / 64]
            }
        }
    };
}

dim!(D1024, 1024, "1024 components (the default, spec §4.5).");
dim!(D2048, 2048, "2048 components.");
dim!(D4096, 4096, "4096 components.");

/// A bipolar hypervector, packed one bit per component: bit `0` is `+1`, bit `1` is `−1`.
/// Multiplication of components is XOR of bits.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Hv<D: Dim> {
    pub(crate) w: D::Words,
    _d: PhantomData<D>,
}

impl<D: Dim> fmt::Debug for Hv<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let w = self.w.as_ref();
        write!(
            f,
            "Hv<{}>({:016x}…)",
            D::BITS,
            w.first().copied().unwrap_or(0)
        )
    }
}

impl<D: Dim> Default for Hv<D> {
    fn default() -> Self {
        Self::ones()
    }
}

impl<D: Dim> Hv<D> {
    pub(crate) fn from_words(w: D::Words) -> Self {
        Self { w, _d: PhantomData }
    }

    /// The all-`+1` vector (identity for [`Hv::bind`]).
    pub fn ones() -> Self {
        Self::from_words(D::zero_words())
    }

    /// Packed words (bit `1` = `−1`), least significant bit of word 0 is component 0.
    pub fn words(&self) -> &[u64] {
        self.w.as_ref()
    }

    /// Component `i` as `+1`/`−1`. Panics are impossible: `i` is reduced modulo the dimension.
    pub fn component(&self, i: u32) -> i8 {
        let i = i % D::BITS;
        let bit = (self.w.as_ref()[(i / 64) as usize] >> (i % 64)) & 1;
        if bit == 0 { 1 } else { -1 }
    }

    /// Bind: component-wise product (XOR). Self-inverse: `a.bind(&b).bind(&b) == a`.
    #[must_use]
    pub fn bind(&self, other: &Self) -> Self {
        let mut w = self.w;
        for (x, y) in w.as_mut().iter_mut().zip(other.w.as_ref()) {
            *x ^= *y;
        }
        Self::from_words(w)
    }

    /// Component-wise negation.
    #[must_use]
    pub fn negate(&self) -> Self {
        let mut w = self.w;
        for x in w.as_mut() {
            *x = !*x;
        }
        Self::from_words(w)
    }

    /// Permute: cyclic rotation by `shift` components (component `i` moves to `i + shift`).
    /// A deliberate symmetry break that encodes order; `permute(D - s)` undoes `permute(s)`.
    #[must_use]
    pub fn permute(&self, shift: u32) -> Self {
        let s = shift % D::BITS;
        if s == 0 {
            return *self;
        }
        let src = self.w.as_ref();
        let n = src.len();
        let (ws, bs) = ((s / 64) as usize, s % 64);
        let mut w = D::zero_words();
        for (k, out) in w.as_mut().iter_mut().enumerate() {
            let lo = src[(k + n - ws) % n];
            *out = if bs == 0 {
                lo
            } else {
                let prev = src[(k + 2 * n - ws - 1) % n];
                (lo << bs) | (prev >> (64 - bs))
            };
        }
        Self::from_words(w)
    }

    /// Number of components that differ.
    pub fn hamming(&self, other: &Self) -> u32 {
        self.w
            .as_ref()
            .iter()
            .zip(other.w.as_ref())
            .map(|(x, y)| (x ^ y).count_ones())
            .sum()
    }

    /// Similarity `(D − 2·hamming)·1000 / D` in [`Millis`] (truncated toward zero): `1000` for
    /// equal vectors, `−1000` for opposite ones, near `0` for unrelated seeded vectors.
    pub fn similarity(&self, other: &Self) -> Millis {
        let d = i32::try_from(D::BITS).unwrap_or(i32::MAX);
        let h = i32::try_from(self.hamming(other)).unwrap_or(i32::MAX);
        Millis::saturating((d - 2 * h) * 1000 / d)
    }
}
