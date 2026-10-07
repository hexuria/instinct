//! Seeded encoder: FNV-1a seed, `SplitMix64` expansion (algorithm credited to hyper-use,
//! re-implemented with Instinct's own tag; spec §4.5).

use crate::hv::{Dim, Hv};

/// Tag mixed into every seed so Instinct vectors never coincide with hyper-use's (`hyper-use-hv1`).
pub const SEED_TAG: &str = "pua-hv1";

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a over each part length-prefixed (`u64` LE) — the same framing as
/// [`instinct_core::DataVersionBuilder::field`]. A single separator byte would let parts
/// containing that byte alias (`[a\x1fb, c]` == `[a, b\x1fc]`); length-prefixing keeps part
/// boundaries unambiguous.
fn fnv1a(parts: &[&[u8]]) -> u64 {
    let mut h = FNV_OFFSET;
    for p in parts {
        for &b in &u64::try_from(p.len()).unwrap_or(u64::MAX).to_le_bytes() {
            h ^= u64::from(b);
            h = h.wrapping_mul(FNV_PRIME);
        }
        for &b in *p {
            h ^= u64::from(b);
            h = h.wrapping_mul(FNV_PRIME);
        }
    }
    h
}

/// `SplitMix64` step (Steele, Lea, Flood 2014).
pub(crate) fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

/// Encodes symbols of one namespace and data version into hypervectors. A pure function of
/// `(tag, namespace, symbol, version)`: nothing is sampled at run time.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Encoder {
    namespace: Box<str>,
    version: u32,
}

impl Encoder {
    /// An encoder for `namespace` at data `version`.
    pub fn new(namespace: &str, version: u32) -> Self {
        Self {
            namespace: namespace.into(),
            version,
        }
    }

    /// The 64-bit seed for `symbol`: FNV-1a over
    /// `len ‖ pua-hv1 ‖ len ‖ namespace ‖ len ‖ symbol ‖ len ‖ version (LE)`, each part
    /// `u64`-LE length-prefixed.
    pub fn seed(&self, symbol: &str) -> u64 {
        fnv1a(&[
            SEED_TAG.as_bytes(),
            self.namespace.as_bytes(),
            symbol.as_bytes(),
            &self.version.to_le_bytes(),
        ])
    }

    /// The hypervector for `symbol`: `SplitMix64` words from [`Encoder::seed`].
    pub fn encode<D: Dim>(&self, symbol: &str) -> Hv<D> {
        let mut state = self.seed(symbol);
        let mut w = D::zero_words();
        for x in w.as_mut() {
            *x = splitmix64(&mut state);
        }
        Hv::from_words(w)
    }

    /// Namespace.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Data version.
    pub fn version(&self) -> u32 {
        self.version
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Raw FNV-1a (no framing) — the oracle the length-prefixed version is checked against.
    fn raw_fnv1a(bytes: &[u8]) -> u64 {
        let mut h = FNV_OFFSET;
        for &b in bytes {
            h ^= u64::from(b);
            h = h.wrapping_mul(FNV_PRIME);
        }
        h
    }

    #[test]
    fn fnv1a_reference_values() {
        // Published FNV-1a 64 test vectors pin the raw hash.
        assert_eq!(raw_fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(raw_fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(raw_fnv1a(b"foobar"), 0x8594_4171_f739_67e8);
        // Parts are u64-LE length-prefixed.
        assert_eq!(fnv1a(&[b"a"]), raw_fnv1a(&[1, 0, 0, 0, 0, 0, 0, 0, b'a']));
        assert_eq!(
            fnv1a(&[b"ab", b"c"]),
            raw_fnv1a(&[
                2, 0, 0, 0, 0, 0, 0, 0, b'a', b'b', 1, 0, 0, 0, 0, 0, 0, 0, b'c'
            ])
        );
        // No aliases: part boundaries are unambiguous.
        assert_ne!(fnv1a(&[b"ab"]), fnv1a(&[b"a", b"b"]));
        assert_ne!(fnv1a(&[b"a\x1fb", b"c"]), fnv1a(&[b"a", b"b\x1fc"]));
        assert_ne!(fnv1a(&[b"a", b"b"]), fnv1a(&[b"ab", b""]));
        assert_eq!(fnv1a(&[]), raw_fnv1a(&[]));
    }

    #[test]
    fn splitmix64_reference_values() {
        // Reference outputs for seed 0 (from the public-domain C implementation).
        let mut s = 0u64;
        assert_eq!(splitmix64(&mut s), 0xe220_a839_7b1d_cdaf);
        assert_eq!(splitmix64(&mut s), 0x6e78_9e6a_a1b9_65f4);
        assert_eq!(splitmix64(&mut s), 0x06c4_5d18_8009_454f);
    }
}
