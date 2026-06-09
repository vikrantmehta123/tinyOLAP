//! Implementing a Mutex from Scratch
//! 
//! A mutex is MUTual EXclusion. It's a lock that guarantees at most
//! one thread will be "inside" the protected region at any instant.
//! Until the current holder holds the lock, everyone else has to wait.
//! 
//! Now, think:
//!     All this mess is to safely share some state between threads.
//!     To safely share the state, we want to "lock" that state.
//!     In theory, we don't need a state- Mutex is about some region
//!     of code. But very often, we have some shared state. Thus, 
//!     we will let Mutex hold some data. This data is the shared
//!     state that we will lock.
//!     If don't have data as part of the mutex, then some other
//!     thread can access the data elsewhere ( not in the locked code
//!     section ) and mutate it. C compiler allows this. Rust doesn't.
//! 
//! We need to provide two distinct guarantees:
//! 1. Exclusivity: At most only one thread holds the lock. 
//! 2. Memory Visibility: When thread A releases the lock and thread 
//!     B acquires it, every write A did before releasing must be 
//!     visible to B after it acquires. Memory ordering 
//! 
//! Once the critical section is over, we need to unlock.
//! In Rust, the idiomatic way to do this will be by implementing
//! the Drop trait.

use std::{cell::UnsafeCell, sync::atomic::{AtomicBool, AtomicUsize, Ordering::{Acquire, Relaxed, Release}}, thread};


struct Mutex<T> {
    flag: AtomicBool, 

    // Why do we need UnsafeCell here? Why can't we simply have T?
    // The idea is: we are going to have multiple threads access this data.
    // And we want them to be able to get mutable reference to this data.
    // Rust's compiler will not allow us to get mutable references across 
    // threads. So we resort to interior mutability. We will internally 
    // mutate this data using shared references. It is upto us to define this
    // implementation: using shared references, safely access/mutate/lock the
    // critical section of the code.
    data: UnsafeCell<T>,
}


/// UnsafeCell<T> is Send only when T is Send.
/// but UnsafeCell<T> is not Sync.
/// We can make Mutex<T> to be Send by adding a qualifier, but we need
/// it to be Sync also.
/// So we do this impl to tell compiler that I know that you can't
/// prove it but I am guaranteeing that it is safe to share the Mutex across
/// threads.
/// 
/// Here, T is Send. But we are adding Sync for the Mutex.
unsafe impl<T: Send> Sync for Mutex<T> {}

/// mutex.lock() returns a MutexGuard
/// 
/// The idea is: we never expose a mutable reference to data from the mutex.
/// We expose `&mut T` only via a guard and only when the guard and lock is 
/// held. We expose a shared reference and the methods to safely access the 
/// data. Thus, if our code guarantees safe access, then the caller cannot 
/// write unsafe code using the Mutex/MutexGuard.
/// 
/// When the guard goes out of scope, Mutex is unlocked.
struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>
}

impl<'a, T> MutexGuard<'a, T> {

    /// We want only one thread to call this. Thus, get(&mut self).
    /// If we do &self, then we have shared references. Then we can do
    ///     let a: &mut T = guard.get();
    ///     let b: &mut T = guard.get();
    /// This breaks the guarantee that only one mutable reference will
    /// be there. So we say, get(&mut self). Rust compiler will ensure
    /// that there is only one exclusive reference for this.
    fn get(&mut self) -> &mut T {

        // UnsafeCell::get() returns a raw, mutable pointer: *mut T.
        // raw pointer and a reference are two different things in Rust.
        // We don't want to expose the raw pointer to the caller.
        // We want them to get the mutable reference.
        // So we need to first dereference the pointer and get the data.
        // Then we need to to do return &mut <dereferenced data>
        unsafe { 
            let raw_ptr = self.mutex.data.get();
            &mut *raw_ptr
        }
    }
}

impl<'a, T> Drop for MutexGuard<'a, T> {
    fn drop(&mut self) {
        // TODO: Understand the Release ordering here.
        self.mutex.flag.store(false, Release);
    }
}

impl<T> Mutex<T> {
    fn new(value: T) -> Self {
        Self {
            flag: AtomicBool::new(false), 
            data: UnsafeCell::new(value),
        }
    }

    fn lock(&self) -> MutexGuard<'_, T> {
        loop {

            // TODO: understand compare_exchange_weak and Acquire ordering
            let res = self.flag.compare_exchange(false, true, Acquire, Relaxed);

            match res {
                Ok(_) => { return MutexGuard { mutex: self } }

                // Busy looping. Ideally, we would perhaps park the thread. But for this
                // implementation, I think busy loop is fine. 
                // So technically, what we have is a SpinLock and not a Mutex. But
                // I am okay with that.
                Err(_) => continue
            }
        }

    }
}

fn main() {
    let m = Mutex::new(0u64);

    // This has nothing to do with Mutex. It's just to verify that only one
    // thread is accessing the critcal section
    let inside = AtomicUsize::new(0);

    thread::scope(|s| {
        s.spawn(||{
            for _ in 0..10_000 {
                let mut g = m.lock();

                // for testing critical section
                let n = inside.fetch_add(1, Relaxed);
                assert_eq!(n, 0, "two threads in the critical section!");

                *g.get() += 1;
                std::thread::sleep(std::time::Duration::from_nanos(1)); // widen the window
                
                // for testing critical section
                inside.fetch_sub(1, Relaxed);
            }
        });

        s.spawn(||{
            for _ in 0..10_000 {
                let mut g = m.lock();

                // for testing critical section
                let n = inside.fetch_add(1, Relaxed);
                assert_eq!(n, 0, "two threads in the critical section!");

                *g.get() += 1;
                std::thread::sleep(std::time::Duration::from_nanos(1)); // widen the window

                // for testing critical section
                inside.fetch_sub(1, Relaxed);
            }
        });
    });

    let total = *m.lock().get();
    assert_eq!(total, 20_000);
    println!("counter = {total}");

}