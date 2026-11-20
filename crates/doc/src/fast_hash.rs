//! [`FastMap`]: a hash map for caches a pass builds and drops, keyed by values this program made
//! (layer ids, variable ids, sets of modes), with a cheaper hash than the standard one.
//!
//! The standard hash resists keys chosen to collide, which a cache that lives for one pass over a
//! document doesn't need: hashing short ids was a large part of applying variable bindings, which a
//! prototype's run does after nearly every action (performance budget PF2). A cache is only ever
//! read by key, never in iteration order, so the hash changes no result.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// A [`HashMap`] hashed with [`FastHasher`].
pub type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FastHasher>>;

/// A multiply-rotate hash over 8-byte words (the scheme of the Rust compiler's own `FxHasher`,
/// written here).
#[derive(Default, Clone, Copy)]
pub struct FastHasher(u64);

const K: u64 = 0x517c_c1b7_2722_0a95;

impl FastHasher {
    fn add(&mut self, word: u64) {
        self.0 = (self.0.rotate_left(5) ^ word).wrapping_mul(K);
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
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::FastMap;

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
