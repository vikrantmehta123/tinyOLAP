//! Building a Threadpool Using Rust Primitives
//! 
//! Most multi-threaded systems will have a threadpool implementation to avoid
//! the cost of spawning threads on each call. At the start of the system, 
//! you spawn the required threads and use them from this pool each time
//! one is needed.

use std::thread::{self, JoinHandle};
use crossbeam_channel::unbounded;

/// A dummy task for the threads to do
fn f(i: i32) {
    println!("{}", i);
}

fn main() {

    // This is how unbounded is defined in the crossbeam_channel crate
    // pub fn unbounded<T>() -> (Sender<T>, Receiver<T>).
    // Crossbeam channels are FIFO ordered. So the order of the task that goes in, 
    // is the order of the tasks in which they are executed.
    // Of course, different tasks may take different amount of time, 
    // but their execution is FIFO

    // For us, we are going to put closures in the channel
    // The main thread will produce tasks (i.e. closures) and put them in the channel
    // The channel mechanism itself will take care of
    // finding the idle thread and that receiver will pick up the task
    // Since we are going to pass a task to be executed once, we set `FnOnce`
    let (sender, receiver) = unbounded::<Box<dyn FnOnce() + Send + 'static>>();

    // Spawn a thread that sends 100 tasks in the channel
    let sender_handle = thread::spawn(move || {
        let mut i = 1;
        loop {
            
            // Sender is moved here. So when closure completes, 
            // the sender will be dropped. No need to manually drop it.
            let _res = sender.send(Box::new(move || f(i))).unwrap();
            i += 1;
            if i == 100 {
                break;
            };
        }
    });
    

    // Create a pool of four threads that are receiving the tasks
    let mut handles: Vec<JoinHandle<()>> = Vec::new();
    for _ in 0..4 {

        let r = receiver.clone();
        let h = thread::spawn( move || {

            // Receiver's `recv` method is not busy-polling. recv() blocks
            // by asking the OS to put it to sleep. When we call send(),
            // crossbeam tells the OS to notify this sleeping thread
            // So if there's not enough work, the threads will stay blocked
            // forever, and will never be called upon to work.
            // `task` is the closure that we passed
            // Loop until senders close the channel or this thread picks up a task.
            loop {
                match r.recv() {
                    Ok(task) => task(),

                    // ThreadId is guaranteed to be unique, but it's not incrementing by one or anything
                    Err(_) => {println!("Channel closed for {:?}", thread::current().id()); break; },
                }
            }
        });
        handles.push(h);
    }

    // Join all handles
    // Note: Without sender_handle completing, the receiver handles cannot complete
    // So sender_handle.join() is redundant here, but kept for the sake of clarity
    // In other implementations this may not necessarily hold
    sender_handle.join().unwrap();
    for h in handles{
        h.join().unwrap();
    }
}