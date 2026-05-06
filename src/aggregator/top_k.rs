//! TopK Frequency Estimation Using Count-Min Sketch
//! 
//! Though this function exists, it is not yet integrated in
//! tinyOLAP. This is considered a "good-to-have" and will 
//! be integrated at a later stage.

use ahash::RandomState;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicU32, Ordering};

pub struct CountMinSketch {
    d: usize,
    w: usize,
    counters: Vec<AtomicU32>,
    hashers: Vec<RandomState>,
}

impl CountMinSketch {
    pub fn with_seeds(d: usize, w: usize, seeds: Vec<[u64; 4]>) -> Self {
        assert!(d > 0, "d must be > 0");
        assert!(w > 0, "w must be > 0");
        assert!(w.is_power_of_two(), "w must be a power of two");
        assert_eq!(seeds.len(), d, "must provide exactly d seed tuples");

        let hashers: Vec<RandomState> = seeds
            .into_iter()
            .map(|[a, b, c, d]| RandomState::with_seeds(a, b, c, d))
            .collect();

        let counters = (0..d * w).map(|_| AtomicU32::new(0)).collect();

        Self { d, w, counters, hashers }
    }

    pub fn new(d: usize, w: usize) -> Self {
        let seeds: Vec<[u64; 4]> = (0..d)
            .map(|_| [rand::random(), rand::random(), rand::random(), rand::random()])
            .collect();
        Self::with_seeds(d, w, seeds)
    }

    pub fn add<T: Hash + ?Sized>(&self, x: &T) {
        let mask = self.w - 1;
        for i in 0..self.d {
            let hash = self.hashers[i].hash_one(x);
            let col = (hash as usize) & mask;
            let cell = &self.counters[i * self.w + col];

            // Saturating increment via CAS loop: stays at u32::MAX once full.
            let mut current = cell.load(Ordering::Relaxed);
            loop {
                if current == u32::MAX {
                    break;
                }
                match cell.compare_exchange_weak(
                    current,
                    current + 1,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => break,
                    Err(actual) => current = actual,
                }
            }
        }
    }

    pub fn estimate<T: Hash + ?Sized>(&self, x: &T) -> u32 {
        let mask = self.w - 1;
        let mut min = u32::MAX;
        for i in 0..self.d {
            let hash = self.hashers[i].hash_one(x);
            let col = (hash as usize) & mask;
            let cell = self.counters[i * self.w + col].load(Ordering::Relaxed);
            if cell < min {
                min = cell;
            }
        }
        min
    }
}

pub struct TopK<T: Hash + Eq + Clone> {
    k: usize,
    sketch: CountMinSketch,
    candidates: HashMap<T, u32>,
}

impl<T: Hash + Eq + Clone> TopK<T> {
    pub fn new(k: usize) -> Self {
        assert!(k > 0, "k must be > 0");
        Self {
            k,
            // d=5 and w=2048 are default, considered-good params for Count-Min sketch
            sketch: CountMinSketch::new(5, 2048), 
            candidates: HashMap::with_capacity(k),
        }
    }

    pub fn add(&mut self, v: &T) {
        self.sketch.add(v);
        let estimate = self.sketch.estimate(v);

        if let Some(count) = self.candidates.get_mut(v) {
            *count = estimate;
            return;
        }

        if self.candidates.len() < self.k {
            self.candidates.insert(v.clone(), estimate);
            return;
        }

        let (min_key, min_count) = self
            .candidates
            .iter()
            .min_by_key(|(_, c)| *c)
            .map(|(k, c)| (k.clone(), *c))
            .unwrap();

        if estimate > min_count {
            self.candidates.remove(&min_key);
            self.candidates.insert(v.clone(), estimate);
        }
    }

    pub fn top(&self) -> Vec<(T, u32)> {
        let mut result: Vec<(T, u32)> = self
            .candidates
            .iter()
            .map(|(k, c)| (k.clone(), *c))
            .collect();
        result.sort_by(|a, b| b.1.cmp(&a.1));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimates_are_at_least_true_count() {
        let seeds = vec![[1, 2, 3, 4], [5, 6, 7, 8], [9, 10, 11, 12]];
        let sketch = CountMinSketch::with_seeds(3, 1024, seeds);

        for _ in 0..100 { sketch.add("apple"); }
        for _ in 0..50  { sketch.add("banana"); }
        sketch.add("cherry");

        assert!(sketch.estimate("apple")  >= 100);
        assert!(sketch.estimate("banana") >= 50);
        assert!(sketch.estimate("cherry") >= 1);
        assert_eq!(sketch.estimate("never_inserted"), 0);
    }

    #[test]
    fn same_seeds_give_same_estimates() {
        let seeds = vec![[1, 2, 3, 4], [5, 6, 7, 8]];
        let a = CountMinSketch::with_seeds(2, 64, seeds.clone());
        let b = CountMinSketch::with_seeds(2, 64, seeds);

        for v in &["x", "y", "z", "x", "x"] {
            a.add(v);
            b.add(v);
        }

        assert_eq!(a.estimate("x"), b.estimate("x"));
        assert_eq!(a.estimate("y"), b.estimate("y"));
    }

    #[test]
    fn concurrent_adds_are_safe_and_correct() {
        use std::sync::Arc;
        use std::thread;

        let sketch = Arc::new(CountMinSketch::new(5, 2048));
        let n_threads = 8;
        let n_per_thread = 1000;

        let handles: Vec<_> = (0..n_threads)
            .map(|_| {
                let s = Arc::clone(&sketch);
                thread::spawn(move || {
                    for _ in 0..n_per_thread {
                        s.add("hot_key");
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        let total = (n_threads * n_per_thread) as u32;
        assert!(sketch.estimate("hot_key") >= total);
    }
}

#[cfg(test)]
mod topk_tests {
    use super::*;

    #[test]
    fn tracks_top_values_by_frequency() {
        let mut topk: TopK<&str> = TopK::new(3);

        for _ in 0..100 { topk.add(&"apple"); }
        for _ in 0..50  { topk.add(&"banana"); }
        for _ in 0..25  { topk.add(&"cherry"); }
        for _ in 0..10  { topk.add(&"date"); }
        for _ in 0..5   { topk.add(&"elder"); }

        let top = topk.top();
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].0, "apple");
        assert_eq!(top[1].0, "banana");
        assert_eq!(top[2].0, "cherry");
        assert!(top[0].1 >= 100);
        assert!(top[1].1 >= 50);
        assert!(top[2].1 >= 25);
    }

    #[test]
    fn returns_fewer_than_k_when_input_is_small() {
        let mut topk: TopK<i64> = TopK::new(5);
        topk.add(&1);
        topk.add(&2);
        topk.add(&2);

        let top = topk.top();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, 2);
        assert_eq!(top[1].0, 1);
    }

    #[test]
    fn evicts_when_a_new_value_beats_the_smallest_tracked() {
        let mut topk: TopK<&str> = TopK::new(2);
        for _ in 0..3  { topk.add(&"a"); }
        for _ in 0..2  { topk.add(&"b"); }
        for _ in 0..10 { topk.add(&"c"); }

        let top = topk.top();
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, "c");
        assert_eq!(top[1].0, "a");
    }
}
