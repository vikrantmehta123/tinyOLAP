//! A Simple Channel From Scratch
//!
//! This bin is to try to create a channel (like mpsc or a crossbeam)
//! from scratch. I want to keep it simple- only single producer and
//! single consumer and only one item. But the core ideas should be
//! there.
//!
//! At the end of the day, a channel is made of the following:
//! 1. a Buffer: to keep the items
//! 2. Shared Access: typically, producer and consumer will be on
//!     different threads, so they need to have a shared access to
//!     the buffer
//! 3. Synchronization: What should the consumer do when buffer is empty?
//!     What should the producer do when the buffer is full? What syscalls
//!     are involved?
//!
//! How do we manage shared access in Rust?
//! Assume we have two threads. Each wants to have a mutable reference to
//! a particular value. Compiler won't let us write this- because every
//! value can have at most one mutable reference or many read-only references.
//! We get around this by having Mutexes & interior mutability. This issue is
//! there for single-thread programs also, which is solved using Cell<T>, but
//! more apparent for multithreaded programs.
//!
//! In this, we don't delve into the syscalls that much behind Mutexes.
//! We assume that a Mutex is a construct given by the language to us and use
//! it.
//!
//! What's a Mutex?
//! Rust will internally call OS's system call for this. OS knows about locking,
//! Rust will wrap it in its type system and make it accessible in programs.
//! It provides the following guarantees:
//!     1. At most one thread will hold the lock for the mutex. Not more.
//!     2. On multicore systems, CPUs can keep local copies of variables. So if
//!         a thread changes a variable's value, there needs be a guarantee that
//!         the other thread won't read the stale value from the CPU's cache, etc.
//!         Mutexes provide this guarantee- there will be no stale reads.
//!     3. In C++, Mutexes don't hold data. So as programmer, we need to remember to
//!         lock and unlock. In Rust, Mutexes are types that own their data. So
//!         the compiler won't allow us to access the data in a Mutex without locking
//!         Unlocking is similar to dropping. When lock's response goes out of scope,
//!         the mutex guard is dropped. Borrow checker prevents dangling pointers too

use std::{
    sync::{Arc, Condvar, Mutex},
    thread,
};

fn main() {
    // For the moment, assume that the buffer is always one integer value.
    // With just a Mutex, we can't make the consumer "wait" for the producer. That means
    // it is possible that the buffer doesn't hold a value- i.e. it will be an Option
    let buffer = Arc::new((Mutex::new(None), Condvar::new()));
    let var1 = buffer.clone();
    let var2 = buffer.clone();

    let producer_handle = thread::spawn(move || {
        let (lock, cvar) = &*var1;
        let mut guard = lock.lock().unwrap();
        *guard = Some(15);

        // wake up one thread waiting in the queue for the Condvar
        // Contrast this with notify_all() which wakes up all the threads.
        // For this example, we know there will only be one thread waiting.
        // Thus notify_one() is fine.
        cvar.notify_one();
    });

    let consumer_handle = thread::spawn(move || {
        // We can have a busy loop: acquire lock, check if value in buffer. If yes, break
        // But the OS provides us with another primitive called Condvar.
        // We want this: until a condition is true, put thread to sleep.
        //               Once the condition is true, wake it up.
        // Condvars provide this.
        let (lock, cvar) = &*var2;
        let mut guard = lock.lock().unwrap();

        // We need to check this in a loop: because it is possible that
        // the OS does a spurious wakeup even when there wasn't a notification
        // and to avoid deadlocks- when consumer starts after the producer has already
        // produced value and there was no-one waiting for it.
        while guard.is_none() {

            // Put the thread in the Condvar's queue and put it to sleep
            guard = cvar.wait(guard).unwrap();
        }

        // We want to "take" the value from the Option and not copy it
        let val = guard.take().unwrap();
        println!("Consumer Got: {}", val);
    });

    producer_handle.join().unwrap();
    consumer_handle.join().unwrap();
}
