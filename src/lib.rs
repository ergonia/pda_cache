#![allow(unexpected_cfgs)]
use solana_program::pubkey::Pubkey;

#[cfg(not(target_os = "solana"))]
mod global {
    use super::*;
    use rustc_hash::{FxBuildHasher, FxHashMap};

    #[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
    struct PdaKey<'a, 'b, 'c> {
        pubkey: &'a [&'b [u8]],
        program_id: &'c Pubkey,
    }

    #[derive(Default)]
    pub(super) struct PdaCache {
        known: parking_lot::RwLock<FxHashMap<PdaKey<'static, 'static, 'static>, (Pubkey, u8)>>,
    }

    impl PdaCache {
        #[cfg(not(target_os = "solana"))]
        pub const fn new() -> Self {
            use parking_lot::RwLock;

            Self {
                known: RwLock::new(FxHashMap::with_hasher(FxBuildHasher)),
            }
        }

        #[cfg(target_os = "solana")]
        pub fn new() -> Self {
            Default::default()
        }

        #[inline(never)]
        pub(super) fn lookup(&self, keys: &[&[u8]], key: &Pubkey) -> (Pubkey, u8) {
            {
                // there is a concept of an ungated read guard, but i don't want to
                // accidentally impact performance by using it on the fast path
                let read_guard = self.known.read();
                let borrowed_key = PdaKey {
                    pubkey: keys,
                    program_id: key,
                };

                if let Some(value) = read_guard.get(&borrowed_key) {
                    return *value;
                }
            }

            let mut write_guard = self.known.write();

            let mut static_keys: Vec<&'static [u8]> = Vec::with_capacity(keys.len());
            for key in keys {
                static_keys.push(Box::leak(key.to_vec().into_boxed_slice()));
            }

            let static_key = PdaKey {
                pubkey: Box::leak(static_keys.into_boxed_slice()),
                program_id: Box::leak(Box::new(*key)),
            };

            let (pda, bump) =
                Pubkey::find_program_address(static_key.pubkey, static_key.program_id);
            if let Some((existing_pda, existing_bump)) = write_guard.insert(static_key, (pda, bump))
            {
                assert_eq!(existing_pda, pda, "Distinct PDAs for the same seeds");
                assert_eq!(existing_bump, bump, "Distinct bumps for the same seeds");
            }
            (pda, bump)
        }
    }

    static GLOBAL_PDA_CACHE: PdaCache = PdaCache::new();

    #[inline]
    pub fn global_pda_cache(keys: &[&[u8]], key: &Pubkey) -> (Pubkey, u8) {
        GLOBAL_PDA_CACHE.lookup(keys, key)
    }
}

#[cfg(not(target_os = "solana"))]
pub use global::global_pda_cache;

#[cfg(target_os = "solana")]
pub fn global_pda_cache(keys: &[&[u8]], key: &Pubkey) -> (Pubkey, u8) {
    panic!("global_pda_cache is not supported on solana - do not call this function onchain");
}

#[cfg(not(target_os = "solana"))]
#[inline]
pub fn global_pda_cache_or_derive(keys: &[&[u8]], key: &Pubkey) -> (Pubkey, u8) {
    global_pda_cache(keys, key)
}

#[cfg(target_os = "solana")]
#[inline]
pub fn global_pda_cache_or_derive(keys: &[&[u8]], key: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(keys, key)
}

#[cfg(test)]
mod tests {
    use super::global::PdaCache;
    use solana_program::pubkey::Pubkey;

    #[test]
    fn test_basic_lookup() {
        let cache = PdaCache::default();
        let program_id = Pubkey::new_unique();
        let seeds: [&[u8]; 2] = [b"test", b"seed"];
        let seed_refs: Vec<&[u8]> = seeds.to_vec();

        let (pda1, bump1) = cache.lookup(&seed_refs, &program_id);
        let (pda2, bump2) = cache.lookup(&seed_refs, &program_id);

        let (expected_pda, expected_bump) = Pubkey::find_program_address(&seed_refs, &program_id);

        assert_eq!(pda1, expected_pda);
        assert_eq!(bump1, expected_bump);
        assert_eq!(pda2, expected_pda);
        assert_eq!(bump2, expected_bump);
    }

    #[test]
    fn test_different_program_ids() {
        let cache = PdaCache::default();
        let program_id1 = Pubkey::new_unique();
        let program_id2 = Pubkey::new_unique();
        let seeds = [b"test", b"seed"];
        let seed_refs: Vec<&[u8]> = seeds.iter().map(|s| s.as_slice()).collect();

        let (expected_pda1, expected_bump1) =
            Pubkey::find_program_address(&seed_refs, &program_id1);
        let (expected_pda2, expected_bump2) =
            Pubkey::find_program_address(&seed_refs, &program_id2);

        let (pda1, bump1) = cache.lookup(&seed_refs, &program_id1);
        let (pda2, bump2) = cache.lookup(&seed_refs, &program_id2);

        assert_ne!(pda1, pda2);

        assert_eq!(pda1, expected_pda1);
        assert_eq!(bump1, expected_bump1);
        assert_eq!(pda2, expected_pda2);
        assert_eq!(bump2, expected_bump2);
    }

    #[test]
    fn test_different_seeds() {
        let cache = PdaCache::default();
        let program_id = Pubkey::new_unique();
        let seeds1: [&[u8]; 2] = [b"test", b"seed1"];
        let seeds2: [&[u8]; 2] = [b"test", b"seed2"];
        let seed_refs1: Vec<&[u8]> = seeds1.to_vec();
        let seed_refs2: Vec<&[u8]> = seeds2.to_vec();

        let (expected_pda1, expected_bump1) =
            Pubkey::find_program_address(&seed_refs1, &program_id);
        let (expected_pda2, expected_bump2) =
            Pubkey::find_program_address(&seed_refs2, &program_id);

        let (pda1, bump1) = cache.lookup(&seed_refs1, &program_id);
        let (pda2, bump2) = cache.lookup(&seed_refs2, &program_id);

        assert_ne!(pda1, pda2);
        assert_eq!(pda1, expected_pda1);
        assert_eq!(bump1, expected_bump1);
        assert_eq!(pda2, expected_pda2);
        assert_eq!(bump2, expected_bump2);
    }

    #[test]
    fn test_empty_seeds() {
        let cache = PdaCache::default();
        let program_id = Pubkey::new_unique();
        let seeds: [&[u8]; 0] = [];
        let seed_refs: Vec<&[u8]> = seeds.to_vec();

        let (pda, bump) = cache.lookup(&seed_refs, &program_id);

        let (expected_pda, expected_bump) = Pubkey::find_program_address(&seed_refs, &program_id);

        assert_eq!(pda, expected_pda);
        assert_eq!(bump, expected_bump);
    }

    #[test]
    fn test_multiple_lookups_same_cache() {
        let cache = PdaCache::default();
        let program_id = Pubkey::new_unique();

        for i in 0..10 {
            let seeds: [&[u8]; 2] = [b"test", &[i as u8]];
            let seed_refs: Vec<&[u8]> = seeds.to_vec();

            let (expected_pda, expected_bump) =
                Pubkey::find_program_address(&seed_refs, &program_id);

            let (pda, bump) = cache.lookup(&seed_refs, &program_id);

            assert_eq!(pda, expected_pda);
            assert_eq!(bump, expected_bump);
        }
    }

    #[test]
    fn test_cache_hit_after_miss() {
        let cache = PdaCache::default();
        let program_id = Pubkey::new_unique();
        let seeds: [&[u8]; 2] = [b"cache", b"test"];
        let seed_refs: Vec<&[u8]> = seeds.to_vec();

        let (expected_pda, expected_bump) = Pubkey::find_program_address(&seed_refs, &program_id);

        // First lookup should miss cache
        let (pda1, bump1) = cache.lookup(&seed_refs, &program_id);

        // Second lookup should hit cache
        let (pda2, bump2) = cache.lookup(&seed_refs, &program_id);

        assert_eq!(pda1, expected_pda);
        assert_eq!(bump1, expected_bump);
        assert_eq!(pda2, expected_pda);
        assert_eq!(bump2, expected_bump);
    }

    #[test]
    fn test_bump_seed_range() {
        let cache = PdaCache::default();
        let program_id = Pubkey::new_unique();
        let seeds: [&[u8]; 2] = [b"bump", b"test"];
        let seed_refs: Vec<&[u8]> = seeds.to_vec();

        let (pda, bump) = cache.lookup(&seed_refs, &program_id);

        let (expected_pda, expected_bump) = Pubkey::find_program_address(&seed_refs, &program_id);

        assert_eq!(pda, expected_pda);
        assert_eq!(bump, expected_bump);
    }
}
