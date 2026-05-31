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
//!         the mutex is is dropped. Borrow checker prevents dangling pointers too
//! 
//! Why do we need an Arc<Mutex>? Why not just Mutex?
//! With Arc, we get shared access. With Mutex, we get to mutate the data. 
//! With just Mutex, when we move the value into the producer thread, then 
//! there is nothing for the consumer thread- the value is moved.
//! 

use std::{sync::{Arc, Mutex}, thread, time::Duration};

fn main() {

    // For the moment, assume that the buffer is always one integer value.
    // With just a Mutex, we can't make the consumer "wait" for the producer. That means
    // it is possible that the buffer doesn't hold a value- i.e. it will be an Option
    let buffer = Arc::new(Mutex::new(None));
    let var1 = buffer.clone();
    let var2 = buffer.clone();

    let producer_handle = thread::spawn(move || {
        let mut guard = var1.lock().unwrap();
        thread::sleep(Duration::from_secs(5));
        *guard = Some(15);

    });
    let consumer_handle = thread::spawn(move ||{

        // Busy loop: acquire lock, check if there's value in the buffer
        // If so, break. Else loop
        loop {
            let guard = var2.lock().unwrap();
            match *guard {
                None => {}, 
                Some(v) => {
                    println!("Consumer Got: {}", v);
                    break;
                }
            }
        }
    });

    producer_handle.join().unwrap();
    consumer_handle.join().unwrap();
}