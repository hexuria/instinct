//! `pua-hdc`: deterministic hyperdimensional computing (spec §4.5).
//!
//! - [`Hv<D>`]: bipolar vectors packed one bit per component, with the dimension a type
//!   ([`D1024`], [`D2048`], [`D4096`]); mixing dimensions does not compile.
//! - [`Encoder`]: FNV-1a seed over `pua-hv1 ‖ namespace ‖ symbol ‖ version`, expanded by
//!   `SplitMix64` (algorithm credited to hyper-use; ported with PUA's own tag, not depended on).
//! - Algebra: [`Hv::bind`] (XOR), [`Hv::permute`] (rotation, the deliberate order-encoding
//!   symmetry break), [`bundle`] / [`Accumulator`] (`i32` sums, zero → `+1`),
//!   [`Hv::similarity`] in `Millis`.
//! - [`Codebook`]: item memory sorted by id; cleanup and iterative top-k decode from the
//!   accumulator.
//! - [`resonate`]: two-factor resonator, at most 16 iterations, else `NotConverged`.
//!
//! No floats, no randomness at run time, no `unsafe`. Capacity is measured, not assumed: see
//! `examples/capacity.rs` and docs/hdc-capacity.md.
//!
//! ```
//! use pua_hdc::{Codebook, D1024, Encoder, Hv, bundle};
//! use pua_core::Millis;
//!
//! let enc = Encoder::new("demo", 1);
//! let [a, b, c]: [Hv<D1024>; 3] = ["alpha", "beta", "gamma"].map(|s| enc.encode(s));
//! assert_eq!(a.similarity(&a).get(), 1000);
//! assert!(a.similarity(&b).get().abs() < 150, "seeded vectors are near-orthogonal");
//! assert_eq!(a.bind(&b).bind(&b), a);
//!
//! let book = Codebook::new([("alpha".into(), a), ("beta".into(), b), ("gamma".into(), c)])?;
//! let noisy = bundle(&[a, a, b]);
//! let hit = book.cleanup(&noisy, Millis::new(300)?).expect("alpha dominates");
//! assert_eq!(book.id(hit.index), "alpha");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![forbid(unsafe_code)]

mod acc;
mod codebook;
mod encode;
mod hv;
mod resonator;

pub use acc::{Accumulator, bundle};
pub use codebook::{Codebook, CodebookError, Nearest};
pub use encode::{Encoder, SEED_TAG};
pub use hv::{D1024, D2048, D4096, Dim, Hv};
pub use resonator::{IterationCap, IterationCapError, MAX_ITERATIONS, Resonance, resonate};

/// Stable identity of the HDC algorithms for `DataVersion` (seed tag, encoder, bundle tie rule,
/// decode rule, resonator cap).
pub const ALGORITHM_TAG: &str = "pua-hdc-v1;seed=pua-hv1;fnv1a64+splitmix64;bundle=i32,zero->+1;decode=l1-norm-dot;resonator<=16";

#[cfg(test)]
mod tests;
