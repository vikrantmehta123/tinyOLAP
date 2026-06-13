//! An Implementation of Swisstables
//!
//! Swisstables are SIMD-friendly hash tables. In this file, I
//! mostly derive the data structure and why it's SIMD friendly.
//! The actual implementation of Swisstable may contain certain
//! other optimizations too that I have not considered.
//! Further, in my implementation there is no resizing. The hash
//! table is fixed size and it will loop forever once it is full.
//! Ideally, we would resize this based on load factor.
//!
//! Traditionally, hashtables handled collisions in one of two ways:
//!     1. Linked Lists
//!     2. Linear Probing
//! There may be other sophisticated ways, but more or less these were
//! dominant. Linked Lists weren't preferred because they lead to a lot
//! of pointer chasing. So most implementations use linear probing.
//!
//! Linear Probing too has problems:
//! 1. Tombstones make lookups expensive:
//!     As the hash table sees churn, it gets a lot of tombstones entries.
//!     The tombstones are flags that say that the current slot is empty for
//!     inserts but for lookups, you should keep on probing. Due to these
//!     tombstones, lookups become a expensive, though tombstones dont
//!     necessarily have impact on inserts. Due to tombstones, the table may
//!     effectively be empty yet the linear probe might need to happen for
//!     a lot of slots.
//! 2. Naive comparison of keys becomes expensive:
//!     When using linear probing, the hashtable needs to compare the key in
//!     slot with   the lookup key. Often, the keys may be strings or such.
//!     Comparing strings isn't cheap. And for linear probing, you may need
//!     to walk and compare a lot before you hit a match. So that's a lot of
//!     wasted effort.
//!
//! Now, swisstable is not a simple data structure. It's a specialized hash
//! table that tries to overcome the above problems by utilising the modern
//! hardware ( and its simd instructions ) and certain other clever things.
//!
//! For this discussion, assume that we resize the capacity after a threshold
//! of load factor and we resize it 2x. So capacity is always a power of two.
//!
//! Now, the key insights that the swisstable is based on.
//! 1. A hash function outputs a 64 bit number. For most cases, several
//!     bits from this hashed output are wasted. Because we take mod of
//!     capacity of array.
//!     Assume that hash % capacity = hash & (capacity - 1). This holds when
//!     capacity is a power of two. There is a mathematical proof for this
//!     but for now, we don't want to get into that. We assume this to be true.
//!     So in theory, only the last few bits are participating in the slot
//!     index calculation- whatever the power of two the capacity is. So
//!     the remaining bits are just there, unutilized.
//! 2. We can use those other bits to solve both of the problems that we
//!     mentioned above. Instead of comparing the whole key, we use a
//!     probabilistic method like bloom filters. So we choose the leftmost
//!     7 bits of the hash. These seven bits is what we call "fingerprint".  
//!     When we do lookup, we compute the hash of the query key. We do
//!     vectorized comparison of the fingerprints by keeping an array of
//!     fingerprints for all the keys in the hashtable.
//!     For each query key, we compute the hash, then we compute
//!     the slot index, and we compute its fingerprint. And then we do SIMD
//!     lookups starting at slot index on the fingerprint till we reach an empty
//!     fingerprint. Whichever fingerprints match, we do naive comparison.
//!     When fingerprints match, it's a possible match- not a guaranteed one.
//!     So we need to do the naive match; but it's much more targeted.
//!     
//! The array of fingerprints is called the control array. It's an array of
//! 8-bit integers. There are three types of 8-bit integers we store in this
//! control array:
//!     FULL: high bit 0, followed by the 7-bit fingerprint.
//!         This indicates that a particular slot is full and it
//!         gives its corresponding fingerprint.
//!     EMPTY: 0xFF or all ones.
//!         This indicates an empty slot. We can use this to terminate
//!         the lookup operation or make an insert.
//!     DELETED: 0x80 or 1000_0000
//!         This indicates a tombstone slot.
//!         
//! Note that we are NOT getting rid of the tombstones. We are just vectorizing
//! the comparisons and making the naive key comparison much more targeted.

#![allow(dead_code)]

use std::{
    arch::x86_64::{__m128i, _mm_cmpeq_epi8, _mm_loadu_si128, _mm_movemask_epi8, _mm_set1_epi8},
    hash::{DefaultHasher, Hash, Hasher},
};

// Consts for states of control array's values
const EMPTY: u8 = 0xFF;
const DELETED: u8 = 0x80;
const INITIAL_HASHTABLE_SIZE: usize = 128;

struct HashMap<K, V>
where
    K: Hash + Eq,
{
    ctrl: Vec<u8>,
    hashtable: Vec<Option<(K, V)>>,
}

impl<K, V> HashMap<K, V>
where
    K: Hash + Eq,
{
    fn new() -> Self {
        Self {
            // Initial hash table capacity is 16 elems
            ctrl: vec![EMPTY; INITIAL_HASHTABLE_SIZE],
            hashtable: (0..INITIAL_HASHTABLE_SIZE).map(|_| None).collect(),
        }
    }

    fn make_hash(&self, key: &K) -> u64 {
        // Uses SipHash hashing algorithm
        let mut hasher = DefaultHasher::new();

        // The key must implement a trait called Hash.
        // The DefaultHasher expects a stream of bytes as input.
        // Any type that implements Hash, knows how to serialize
        // itself to the hasher. That's what we do here.
        // We ask the key to serialize itself into a stream of
        // bytes and pass those in the hasher.
        key.hash(&mut hasher);

        // Get the hashed value from the streamed in bytes.
        hasher.finish()
    }

    /// How should the get() algorithm work in vectorized case?
    /// 
    /// We iterate over elements one group at a time. Each group is 16-bytes.
    /// That is, we look at 16 fingerprints at a time.
    /// 
    /// First, we round down to the nearest 16's multiple. Hashbrown doesn't do
    /// this. But it's a simplification we have done. From there, we look at
    /// 16 fingerprints at a time.
    /// 
    /// Wherever we get a fingerprint match, we do the naive match.
    /// Once we get the EMPTY fingerprint, we terminate by saying that
    /// key not found.
    fn get(&self, key: &K) -> Option<&V> {
        let hash = self.make_hash(&key);

        let capacity = self.ctrl.len() as u64;
        let idx = (hash & (capacity - 1u64)) as usize;

        // We need to select the leftmost 7 bits
        // So push the 64 bits right by 57 bits.
        // We get: 000...<57 zeros>...< leftmost 7 bits >
        // We can cast these as u8. This becomes our fingerprint.
        // The fingerprint byte is: 0<7bits> => FULL control byte
        // That is what we want.
        let fingerprint = (hash >> 57) as u8;

        // round down to nearest 16's multiple
        let mut group_start: usize = idx & !(16 - 1);

        loop {
            unsafe {
                let v = _mm_loadu_si128(self.ctrl.as_ptr().add(group_start) as *const __m128i);
                let broadcast = _mm_set1_epi8(fingerprint as i8);
                let comparison_result = _mm_cmpeq_epi8(v, broadcast);
                let selection = _mm_movemask_epi8(comparison_result) as u16;

                let mut mask = selection;

                // Check for potential matches
                while mask != 0 {
                    let lane = mask.trailing_zeros() as usize;
                    let slot = group_start + lane;
                    
                    // Do naive full matching
                    if self.hashtable[slot].as_ref().unwrap().0 == *key {
                        return Some(&self.hashtable[slot].as_ref().unwrap().1);
                    }
                    mask &= mask - 1; // clears the lowest set bit
                }

                // termination condition check
                let empty_broadcast = _mm_set1_epi8(EMPTY as i8);
                let empty_cmp = _mm_cmpeq_epi8(v, empty_broadcast); // reuse v
                let empty_mask = _mm_movemask_epi8(empty_cmp) as u16;

                if empty_mask != 0 {
                    return None;
                }

                // Wrap around
                group_start = (group_start + 16) as usize & (capacity - 1) as usize;
            }
        }
    }


    /// How do we expect the insert operation to behave?
    /// 
    /// We want two things:
    ///     1. If the key is already present, then replace.
    ///     2. If not, find a slot for it and insert the fingerprint
    ///         and the key-value pair.
    /// 
    /// Checking whether the key is already present is similar to the 
    /// get operation above. Fairly straightforward.
    /// 
    /// Let's say the key didn't exist and we need to find a new slot for it.
    /// How do we find it?
    /// It's either the first tombstone that we see OR it's the first EMPTY
    /// slot. So we need to keep track of the first tombstone that we see.
    /// 
    /// Whenever we see an EMPTY slot, that's the termination condition. At
    /// this point, we will either insert (K,V) in that EMPTY slot or if we
    /// saw a tombstone before, we will insert it there. But the algorithm
    /// stops when we encounter EMPTY slot.
    fn insert(&mut self, key: K, value: V) {
        let hash = self.make_hash(&key);
        let capacity = self.ctrl.len() as u64;
        let idx = (hash & (capacity - 1u64)) as usize;
        let fingerprint = (hash >> 57) as u8;

        let mut group_start = idx & !(16 - 1);

        // Keep track of the index of the first tombstone that we see
        let mut first_tombstone: Option<usize> = None;

        unsafe {
            loop {
                let v = _mm_loadu_si128(self.ctrl.as_ptr().add(group_start) as *const __m128i);
                let broadcast = _mm_set1_epi8(fingerprint as i8);
                let comparison_result = _mm_cmpeq_epi8(v, broadcast);
                let selection = _mm_movemask_epi8(comparison_result) as u16;
                let mut mask = selection;

                // Check the potential matches to see if the key already exists.
                // If so, replace. No need to replace the fingerprint in control
                // array as it will be the same.
                while mask != 0 {
                    let lane = mask.trailing_zeros() as usize;
                    let slot = group_start + lane;

                    if self.hashtable[slot].as_ref().unwrap().0 == key {
                        self.hashtable[slot] = Some((key, value));
                        return;
                    }
                    mask &= mask - 1;
                }

                // Check for the DELETED slot. This is to populate the first tombstone value
                // We need to populate this only once.
                let del_cmp = _mm_cmpeq_epi8(v, _mm_set1_epi8(DELETED as i8));
                let del_mask = _mm_movemask_epi8(del_cmp) as u16;
                if first_tombstone.is_none() && del_mask != 0 {
                    first_tombstone = Some(group_start + del_mask.trailing_zeros() as usize);
                }

                // Termination condition
                let empty_broadcast = _mm_set1_epi8(EMPTY as i8);
                let empty_cmp = _mm_cmpeq_epi8(v, empty_broadcast); 
                let empty_mask = _mm_movemask_epi8(empty_cmp) as u16;

                // Either we insert at the empty slot or at the first tombstone slot
                if empty_mask != 0 {
                    let slot = first_tombstone
                        .unwrap_or(group_start + empty_mask.trailing_zeros() as usize);

                    self.ctrl[slot] = fingerprint;
                    self.hashtable[slot] = Some((key, value));
                    return;
                }
                group_start = (group_start + 16) as usize & (capacity - 1) as usize;
            }
        }
    }

    /// How do we delete an element? What's the expected end state?
    /// If the key existed:
    ///     - We want to make hashtable[slot] = None.
    ///     - And, we want to insert a ctrl[slot] = DELETED
    /// If the key didn't exist, we don't have to do anything.
    ///
    /// So we first find whether the key existed or not.
    /// We do this by doing the vector comparisons of the control array.
    /// Once a potential match is found, we do a naive comparison.
    /// If match is confirmed, we do the above things.
    ///
    /// Finding an EMPTY slot is the termination condition, which means
    /// that the key was not found.
    fn delete(&mut self, key: &K) {
        let hash = self.make_hash(&key);
        let capacity = self.ctrl.len() as u64;
        let idx = (hash & (capacity - 1u64)) as usize;
        let fingerprint = (hash >> 57) as u8;

        let mut group_start: usize = idx & !(16 - 1);

        loop {
            unsafe {
                // Compare the group with the fingerprint and look for a match
                let v = _mm_loadu_si128(self.ctrl.as_ptr().add(group_start) as *const __m128i); // load
                let broadcast = _mm_set1_epi8(fingerprint as i8); // broadcast fingerprint
                let comparison_result = _mm_cmpeq_epi8(v, broadcast); // compare
                let selection = _mm_movemask_epi8(comparison_result) as u16; // Convert 128-bit "lanes output" into a 16-bit number

                let mut mask = selection;
                while mask != 0 {
                    let lane = mask.trailing_zeros() as usize;
                    let slot = group_start + lane;

                    if self.hashtable[slot].as_ref().unwrap().0 == *key {
                        self.hashtable[slot] = None;
                        self.ctrl[slot] = DELETED;
                        return;
                    }
                    mask &= mask - 1;
                }

                // Termination condition: element not found.
                let empty_broadcast = _mm_set1_epi8(EMPTY as i8);
                let empty_cmp = _mm_cmpeq_epi8(v, empty_broadcast); // reuse v
                let empty_mask = _mm_movemask_epi8(empty_cmp) as u16;

                if empty_mask != 0 {
                    return;
                }
                group_start = (group_start + 16) as usize & (capacity - 1) as usize;
            }
        }
    }
}

fn main() {}

/// Claude-generated test suite.
///
/// Scope: the vectorized, multi-group SwissTable — capacity = 128 (8 groups of
/// 16), operations `insert` / `get` / `delete`, with overwrite and tombstone
/// reuse. Resizing is NOT implemented, so the table is permanently 128 slots.
///
/// INVARIANT FOR EVERY TEST: at least one EMPTY slot must remain in any probe
/// path, otherwise a probe over a fully-occupied table spins forever. So we
/// keep (live + tombstones) comfortably below 128.
#[cfg(test)]
mod tests {
    use super::*;

    // ---- helpers ----

    /// FULL slots: high bit clear.
    fn full_slots<K: Hash + Eq, V>(m: &HashMap<K, V>) -> usize {
        m.ctrl.iter().filter(|&&b| b & 0x80 == 0).count()
    }
    fn tombstones<K: Hash + Eq, V>(m: &HashMap<K, V>) -> usize {
        m.ctrl.iter().filter(|&&b| b == DELETED).count()
    }
    fn empties<K: Hash + Eq, V>(m: &HashMap<K, V>) -> usize {
        m.ctrl.iter().filter(|&&b| b == EMPTY).count()
    }
    fn s(x: &str) -> String {
        x.to_string()
    }

    // ================= 1. empty / freshly constructed table =================

    #[test]
    fn new_table_is_all_empty() {
        let m: HashMap<String, u64> = HashMap::new();
        assert_eq!(m.ctrl.len(), 128);
        assert_eq!(empties(&m), 128);
        assert_eq!(full_slots(&m), 0);
        assert_eq!(tombstones(&m), 0);
    }

    #[test]
    fn get_on_empty_table_is_none() {
        let m: HashMap<String, u64> = HashMap::new();
        assert_eq!(m.get(&s("anything")), None);
    }

    #[test]
    fn delete_on_empty_table_is_noop() {
        let mut m: HashMap<String, u64> = HashMap::new();
        m.delete(&s("anything")); // must not panic or hang
        assert_eq!(full_slots(&m), 0);
        assert_eq!(tombstones(&m), 0);
    }

    // ===================== 2. single insert / get =====================

    #[test]
    fn insert_then_get() {
        let mut m = HashMap::new();
        m.insert(s("apple"), 1u64);
        assert_eq!(m.get(&s("apple")), Some(&1));
    }

    #[test]
    fn insert_claims_exactly_one_slot() {
        let mut m = HashMap::new();
        m.insert(s("apple"), 1u64);
        assert_eq!(full_slots(&m), 1);
        assert_eq!(empties(&m), 127);
    }

    #[test]
    fn get_absent_key_after_insert_is_none() {
        let mut m = HashMap::new();
        m.insert(s("apple"), 1u64);
        assert_eq!(m.get(&s("zebra")), None);
    }

    // ========================= 3. overwrite =========================

    #[test]
    fn overwrite_updates_value() {
        let mut m = HashMap::new();
        m.insert(s("apple"), 1u64);
        m.insert(s("apple"), 42);
        assert_eq!(m.get(&s("apple")), Some(&42));
    }

    #[test]
    fn overwrite_claims_no_new_slot() {
        let mut m = HashMap::new();
        m.insert(s("apple"), 1u64);
        let ctrl_before = m.ctrl.clone();
        m.insert(s("apple"), 42);
        assert_eq!(m.ctrl, ctrl_before);
        assert_eq!(full_slots(&m), 1);
    }

    #[test]
    fn repeated_overwrite_latest_wins() {
        let mut m = HashMap::new();
        for v in 0..50u64 {
            m.insert(s("apple"), v);
        }
        assert_eq!(m.get(&s("apple")), Some(&49));
        assert_eq!(full_slots(&m), 1);
    }

    // ============ 4. many keys / multi-group / collisions ============

    #[test]
    fn many_keys_round_trip() {
        // 100 keys force spill across several of the 8 groups.
        let mut m = HashMap::new();
        for i in 0..100u64 {
            m.insert(format!("key{i}"), i);
        }
        for i in 0..100u64 {
            assert_eq!(m.get(&format!("key{i}")), Some(&i), "key{i} lost");
        }
    }

    #[test]
    fn full_count_equals_distinct_keys() {
        let mut m = HashMap::new();
        for i in 0..100u64 {
            m.insert(format!("key{i}"), i);
        }
        assert_eq!(full_slots(&m), 100);
    }

    #[test]
    fn each_key_keeps_its_own_value() {
        // Cross-talk guard: a mis-probe would return a neighbor's value.
        let mut m = HashMap::new();
        for i in 0..90u64 {
            m.insert(format!("k{i}"), 1000 + i);
        }
        for i in 0..90u64 {
            assert_eq!(m.get(&format!("k{i}")), Some(&(1000 + i)));
        }
    }

    #[test]
    fn near_capacity_round_trip_and_miss() {
        // 110 of 128: heavy load, lots of spill + wraparound, 18 EMPTY left.
        let mut m = HashMap::new();
        for i in 0..110u64 {
            m.insert(format!("key{i}"), i * 7);
        }
        for i in 0..110u64 {
            assert_eq!(m.get(&format!("key{i}")), Some(&(i * 7)));
        }
        assert_eq!(m.get(&s("absent")), None);
    }

    #[test]
    fn insertion_order_does_not_matter() {
        let mut forward = HashMap::new();
        for i in 0..60u64 {
            forward.insert(format!("key{i}"), i);
        }
        let mut reverse = HashMap::new();
        for i in (0..60u64).rev() {
            reverse.insert(format!("key{i}"), i);
        }
        for i in 0..60u64 {
            assert_eq!(
                forward.get(&format!("key{i}")),
                reverse.get(&format!("key{i}"))
            );
        }
    }

    // ============================ 5. misses ============================

    #[test]
    fn miss_on_populated_table() {
        let mut m = HashMap::new();
        m.insert(s("apple"), 1u64);
        m.insert(s("banana"), 2);
        assert_eq!(m.get(&s("cherry")), None);
    }

    #[test]
    fn misses_terminate_under_heavy_load() {
        // A miss may have to walk several full groups (wrapping) before it
        // finds an EMPTY lane and gives up. Would hang if termination broke.
        let mut m = HashMap::new();
        for i in 0..110u64 {
            m.insert(format!("present{i}"), i);
        }
        for i in 0..50u64 {
            assert_eq!(m.get(&format!("absent{i}")), None, "absent{i} found");
        }
    }

    // ========================= 6. delete basics =========================

    #[test]
    fn delete_then_get_is_none() {
        let mut m = HashMap::new();
        m.insert(s("apple"), 1u64);
        m.delete(&s("apple"));
        assert_eq!(m.get(&s("apple")), None);
    }

    #[test]
    fn delete_writes_tombstone_not_empty() {
        let mut m = HashMap::new();
        m.insert(s("apple"), 1u64);
        let empties_before = empties(&m);
        m.delete(&s("apple"));
        // Slot becomes DELETED, not EMPTY: full count drops, tombstone count
        // rises, and the EMPTY count is unchanged.
        assert_eq!(full_slots(&m), 0);
        assert_eq!(tombstones(&m), 1);
        assert_eq!(empties(&m), empties_before);
    }

    #[test]
    fn delete_missing_key_is_noop() {
        let mut m = HashMap::new();
        m.insert(s("apple"), 1u64);
        m.delete(&s("zebra"));
        assert_eq!(m.get(&s("apple")), Some(&1));
        assert_eq!(tombstones(&m), 0);
    }

    #[test]
    fn double_delete_is_safe() {
        let mut m = HashMap::new();
        m.insert(s("apple"), 1u64);
        m.delete(&s("apple"));
        m.delete(&s("apple")); // already a tombstone
        assert_eq!(m.get(&s("apple")), None);
        assert_eq!(tombstones(&m), 1);
    }

    #[test]
    fn delete_preserves_other_key() {
        let mut m = HashMap::new();
        m.insert(s("apple"), 1u64);
        m.insert(s("banana"), 2);
        m.delete(&s("apple"));
        assert_eq!(m.get(&s("apple")), None);
        assert_eq!(m.get(&s("banana")), Some(&2));
    }

    #[test]
    fn delete_preserves_survivors_across_groups() {
        // The defining tombstone test: delete every other key, survivors must
        // still be reachable. If delete wrote EMPTY it would sever a probe
        // chain and lose a survivor.
        let mut m = HashMap::new();
        for i in 0..100u64 {
            m.insert(format!("key{i}"), i);
        }
        for i in (0..100u64).step_by(2) {
            m.delete(&format!("key{i}"));
        }
        for i in 0..100u64 {
            if i % 2 == 0 {
                assert_eq!(m.get(&format!("key{i}")), None, "key{i} should be gone");
            } else {
                assert_eq!(m.get(&format!("key{i}")), Some(&i), "key{i} lost");
            }
        }
    }

    // =============== 7. delete + reinsert / churn / reuse ===============

    #[test]
    fn delete_then_reinsert_sees_new_value() {
        let mut m = HashMap::new();
        m.insert(s("apple"), 1u64);
        m.delete(&s("apple"));
        m.insert(s("apple"), 99);
        assert_eq!(m.get(&s("apple")), Some(&99));
    }

    #[test]
    fn churn_reuses_tombstones_without_wedging() {
        // THE tombstone-reuse test. Each cycle inserts 40 keys then deletes
        // them. Without reuse, each re-insert consumes a fresh EMPTY while old
        // slots stay DELETED, so the 128-slot table exhausts empties after a
        // few cycles and `insert` spins forever. With reuse it runs forever.
        let mut m = HashMap::new();
        for cycle in 0..100u64 {
            for i in 0..40u64 {
                m.insert(format!("k{i}"), cycle * 1000 + i);
            }
            for i in 0..40u64 {
                assert_eq!(m.get(&format!("k{i}")), Some(&(cycle * 1000 + i)));
            }
            assert_eq!(full_slots(&m), 40, "cycle {cycle}: slot leak");
            for i in 0..40u64 {
                m.delete(&format!("k{i}"));
            }
            assert_eq!(full_slots(&m), 0, "cycle {cycle}: keys lingered");
        }
    }

    #[test]
    fn interleaved_mixed_operations() {
        // A hand-rolled sequence mixing insert / overwrite / delete / get.
        let mut m = HashMap::new();
        m.insert(s("a"), 1u64);
        m.insert(s("b"), 2);
        m.insert(s("c"), 3);
        assert_eq!(m.get(&s("b")), Some(&2));

        m.delete(&s("b"));
        assert_eq!(m.get(&s("b")), None);
        assert_eq!(m.get(&s("a")), Some(&1));
        assert_eq!(m.get(&s("c")), Some(&3));

        m.insert(s("a"), 100); // overwrite
        m.insert(s("d"), 4); // new
        m.insert(s("b"), 22); // reinsert deleted

        assert_eq!(m.get(&s("a")), Some(&100));
        assert_eq!(m.get(&s("b")), Some(&22));
        assert_eq!(m.get(&s("c")), Some(&3));
        assert_eq!(m.get(&s("d")), Some(&4));
    }

    // ===================== 8. control-byte invariants =====================

    #[test]
    fn insert_only_ctrl_is_full_or_empty() {
        // With no deletions there must be no DELETED bytes at all.
        let mut m = HashMap::new();
        for i in 0..50u64 {
            m.insert(format!("key{i}"), i);
        }
        for &b in &m.ctrl {
            assert!(b & 0x80 == 0 || b == EMPTY, "unexpected ctrl byte {b:#04x}");
        }
        assert_eq!(tombstones(&m), 0);
    }

    #[test]
    fn after_deletes_ctrl_is_one_of_three_states() {
        // Every byte must be FULL (high bit 0), EMPTY, or DELETED — nothing else.
        let mut m = HashMap::new();
        for i in 0..50u64 {
            m.insert(format!("key{i}"), i);
        }
        for i in (0..50u64).step_by(3) {
            m.delete(&format!("key{i}"));
        }
        for &b in &m.ctrl {
            let valid = b & 0x80 == 0 || b == EMPTY || b == DELETED;
            assert!(valid, "unexpected ctrl byte {b:#04x}");
        }
        assert!(tombstones(&m) > 0, "expected some tombstones");
    }

    // ================= 9. key & value type generality =================

    #[test]
    fn integer_keys_round_trip() {
        let mut m: HashMap<u64, u64> = HashMap::new();
        for i in 0..100u64 {
            m.insert(i, i * i);
        }
        for i in 0..100u64 {
            assert_eq!(m.get(&i), Some(&(i * i)));
        }
        assert_eq!(m.get(&999), None);
    }

    #[test]
    fn tuple_keys_round_trip() {
        let mut m: HashMap<(u32, u32), u64> = HashMap::new();
        for i in 0..50u32 {
            m.insert((i, i + 1), i as u64);
        }
        for i in 0..50u32 {
            assert_eq!(m.get(&(i, i + 1)), Some(&(i as u64)));
        }
        assert_eq!(m.get(&(0, 0)), None);
    }

    #[test]
    fn non_copy_values_round_trip() {
        // String values exercise the ownership-move paths in insert (overwrite
        // must drop the old String) and confirm get hands back the right one.
        let mut m: HashMap<u64, String> = HashMap::new();
        for i in 0..40u64 {
            m.insert(i, format!("value-{i}"));
        }
        m.insert(7, s("overwritten"));
        for i in 0..40u64 {
            let expected = if i == 7 {
                s("overwritten")
            } else {
                format!("value-{i}")
            };
            assert_eq!(m.get(&i), Some(&expected));
        }
    }
}
