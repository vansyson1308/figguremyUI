//! [`FastMap`]: a hash map for caches a pass builds and drops (layer ids, variable ids, sets of
//! modes), with a cheaper hash than the standard one.
//!
//! Hashing short ids with the standard hash was a large part of applying variable bindings, which a
//! prototype's run does after nearly every action (performance budget PF2). The keys come from the
//! file (a document's layer and variable ids are read as they are written), and a file is hostile
//! input: with a fixed hash, a file could choose ids that all land in one slot and make each insert
//! walk past every other (a denial of service). So the hash is keyed with secret values drawn once
//! per process from the standard library's own random source (`RandomState`), each 8-byte word is
//! mixed in through a multiply with a key, and the result goes through a full-avalanche finish (every
//! bit of the state changes every bit of the hash with even odds), so which keys collide can't be
//! worked out from outside. A cache is only ever read by key, never in iteration order, so the
//! hash, and its seed, change no result.

use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, BuildHasherDefault, Hasher};
use std::sync::OnceLock;

/// A [`HashMap`] hashed with [`FastHasher`].
pub type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

/// A keyed hash over 8-byte words: each word is folded in by a 64 × 64 → 128-bit multiply with a
/// secret key (the high and low halves xored), and [`finish`](Hasher::finish) mixes the state with
/// a full-avalanche finaliser. The keys are this process's ([`keys`]); a default hasher starts
/// from them.
#[derive(Clone, Copy)]
pub struct FastHasher {
    state: u64,
    key: u64,
    out: u64,
}

impl Default for FastHasher {
    fn default() -> Self {
        let [start, key, out] = *keys();
        Self { state: start, key, out }
    }
}

/// This process's three keys, drawn once from [`RandomState`] (the source the standard hash map
/// seeds itself from; on a target with no random source it is what the standard map gets too).
fn keys() -> &'static [u64; 3] {
    static KEYS: OnceLock<[u64; 3]> = OnceLock::new();
    KEYS.get_or_init(|| {
        let random = RandomState::new();
        let draw = |i: u64| random.hash_one(i);
        // The multiplier is kept odd, so no word can be multiplied away to nothing.
        [draw(0), draw(1) | 1, draw(2)]
    })
}

/// `a × b` as 128 bits, its two halves xored: every bit of `a` reaches the low half.
fn fold(a: u64, b: u64) -> u64 {
    let p = u128::from(a) * u128::from(b);
    (p as u64) ^ ((p >> 64) as u64)
}

/// The 64-bit finaliser of MurmurHash3 (public domain): a bijection with full avalanche.
fn avalanche(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ce_b9fe_1a85_ec53);
    x ^ (x >> 33)
}

impl FastHasher {
    fn add(&mut self, word: u64) {
        self.state = fold(self.state ^ word, self.key);
    }
}

impl Hasher for FastHasher {
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            let mut w = [0u8; 8];
            w.copy_from_slice(c);
            self.add(u64::from_le_bytes(w));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut w = [0u8; 8];
            if let Some(dst) = w.get_mut(..rest.len()) {
                dst.copy_from_slice(rest);
            }
            self.add(u64::from_le_bytes(w));
        }
    }

    fn write_u8(&mut self, i: u8) {
        self.add(u64::from(i));
    }

    fn write_u32(&mut self, i: u32) {
        self.add(u64::from(i));
    }

    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }

    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    fn finish(&self) -> u64 {
        avalanche(self.state ^ self.out)
    }
}

#[cfg(test)]
mod tests {
    use super::FastMap;
    use std::hash::{BuildHasher, Hasher};

    /// The most keys of `hashes` that share a slot of a table of 2^16 slots (a hash table picks a
    /// key's slot from the hash's low bits): keys spread at random put about 1 in each, a dozen at
    /// most; keys made to collide put them all in one, and every insert then walks past the others.
    fn most_in_one_slot(hashes: impl Iterator<Item = u64>) -> usize {
        let mut slots = vec![0usize; 1 << 16];
        for h in hashes {
            if let Some(s) = slots.get_mut((h & 0xFFFF) as usize) {
                *s += 1;
            }
        }
        slots.into_iter().max().unwrap_or(0)
    }

    /// `x`'s inverse modulo 2^64 (`x` odd).
    fn inverse(x: u64) -> u64 {
        let mut inv = x;
        for _ in 0..6 {
            inv = inv.wrapping_mul(2u64.wrapping_sub(x.wrapping_mul(inv)));
        }
        inv
    }

    #[test]
    fn layer_ids_chosen_to_collide_spread_like_any_others() {
        // A file chooses its layer ids. Against a multiply-only hash with a known constant, ids
        // `(i << 24) · K⁻¹` hash to `i << 24`: all of them land in one slot.
        let k_inv = inverse(0x517c_c1b7_2722_0a95);
        let crafted: Vec<crate::NodeId> = (1..=50_000u64).map(|i| crate::NodeId((i << 24).wrapping_mul(k_inv))).collect();
        let m: FastMap<crate::NodeId, ()> = FastMap::default();
        let most = most_in_one_slot(crafted.iter().map(|k| m.hasher().hash_one(k)));
        assert!(most <= 32, "{most} of 50,000 crafted layer ids share one slot");
        // And they go in and come out as fast as ordinary ones would (it is the slots, above,
        // that decides that; this checks the map with them).
        let mut map: FastMap<crate::NodeId, usize> = FastMap::default();
        for (i, k) in crafted.iter().enumerate() {
            map.insert(*k, i);
        }
        assert!(crafted.iter().enumerate().all(|(i, k)| map.get(k) == Some(&i)));
    }

    #[test]
    fn strings_chosen_to_collide_spread_like_any_others() {
        // A file chooses its variable ids. Against a hash that mixes each 8-byte word with a known
        // constant, the second word of a 16-byte id can be picked to cancel the first, so every id
        // reaches the same state: a full 64-bit collision. (Bytes as a `str` hashes them: its bytes,
        // then 0xFF.)
        const K: u64 = 0x517c_c1b7_2722_0a95;
        let same = 0x0123_4567_89AB_CDEFu64;
        let crafted: Vec<[u8; 16]> = (1..=50_000u64)
            .map(|i| {
                let first = i.wrapping_mul(K).rotate_left(5);
                let mut b = [0u8; 16];
                b[..8].copy_from_slice(&i.to_le_bytes());
                b[8..].copy_from_slice(&(same ^ first).to_le_bytes());
                b
            })
            .collect();
        let m: FastMap<String, ()> = FastMap::default();
        let hash = |b: &[u8; 16]| {
            let mut h = m.hasher().build_hasher();
            h.write(b);
            h.write_u8(0xFF);
            h.finish()
        };
        let hashes: Vec<u64> = crafted.iter().map(hash).collect();
        let distinct: std::collections::BTreeSet<u64> = hashes.iter().copied().collect();
        assert!(distinct.len() >= 49_990, "{} distinct hashes of 50,000 crafted ids", distinct.len());
        let most = most_in_one_slot(hashes.into_iter());
        assert!(most <= 32, "{most} of 50,000 crafted ids share one slot");
    }

    #[test]
    fn the_hash_is_keyed_per_process() {
        // Not a constant anyone can work against: the same key hashes the same within a process
        // (a map finds it) and differently from a hash with no key (no seed, no final mix).
        let m: FastMap<u64, ()> = FastMap::default();
        let a = m.hasher().hash_one(7u64);
        assert_eq!(a, FastMap::<u64, ()>::default().hasher().hash_one(7u64));
        assert_ne!(a, 7u64.wrapping_mul(0x517c_c1b7_2722_0a95));
    }

    #[test]
    fn a_fast_map_finds_what_was_put_in_it() {
        let mut m: FastMap<String, usize> = FastMap::default();
        let keys: Vec<String> = (0..500).map(|i| format!("VariableID:{i}")).chain(["".into(), "a".into(), "12345678".into()]).collect();
        for (i, k) in keys.iter().enumerate() {
            m.insert(k.clone(), i);
        }
        for (i, k) in keys.iter().enumerate() {
            assert_eq!(m.get(k.as_str()), Some(&i));
        }
        assert_eq!(m.get("VariableID:500"), None);
        let mut ids: FastMap<crate::NodeId, u8> = FastMap::default();
        ids.insert(crate::NodeId(7), 1);
        assert_eq!(ids.get(&crate::NodeId(7)), Some(&1));
        assert_eq!(ids.get(&crate::NodeId(8)), None);
    }
}
