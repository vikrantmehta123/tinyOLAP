//! WorkSource trait and its implementation
//!
//! Each scan operator asks the work source for the next piece of work.
//! This could be a part or a granule or anything. The WorkSource hands
//! out some identifier for the work and the Scan operator do the actual
//! work.
//!
//! This is required because we want parallel execution. When we have
//! parallel scanning, we need the threads to be able to know what
//! units are to be read next and avoid duplication of work.

use std::{
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering::Relaxed},
};

/// This trait defines an abstraction for the FullScan or ZoneMapScan
/// operators. Because of this trait, those operators don't have to keep
/// track of what parts are read.
/// Since main purpose of this trait is to enable parallel scans,
/// it needs to be Send and Sync.
pub trait ScanWorkSource: Send + Sync {
    fn next_work(&self) -> Option<PathBuf>;
}

pub struct PartWorkSource {
    parts: Vec<PathBuf>,
    next: AtomicUsize, // The index in the parts vector upto which workers have read
}

/// For the moment, we have a PartWorkSource.
/// This represents a Handle that the workers use when
/// executing the Scan operator
impl PartWorkSource {
    pub fn new(parts: Vec<PathBuf>) -> Self {
        Self {
            parts,
            next: AtomicUsize::new(0),
        }
    }
}

impl ScanWorkSource for PartWorkSource {
    fn next_work(&self) -> Option<PathBuf> {
        let val = self.next.fetch_add(1, Relaxed);
        self.parts.get(val).cloned()
    }
}

// TODO: Implement GranuleWorkSource.
// The reason we want this to be at a granule level is that if 
// the parts are large, then parallelization doesn't help because
// the OS is going to start evicting the parts from memory
// So we can't increase the channel size to more than 1.
// Thus we need the scans to be parallelized at a granule level
//
// pub struct GranuleWorkSource {}
