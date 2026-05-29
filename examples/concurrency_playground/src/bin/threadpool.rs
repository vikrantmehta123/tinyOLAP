//! Building a Threadpool Using Rust Primitives
//! 
//! Most multi-threaded systems will have a threadpool implementation to avoid
//! the cost of spawning threads on each call. At the start of the system, 
//! you spawn the required threads and use them from this pool each time
//! one is needed.

use std::thread::{self, JoinHandle};
use crossbeam_channel::{Sender, unbounded};

type Task = Box<dyn FnOnce() + Send + 'static>;

/// A dummy task for the threads to do
fn f(i: i32) {
    println!("{}", i);
}

struct ThreadPool {
    handles: Vec<JoinHandle<()>>,
    sender: Sender<Task>,
}

impl ThreadPool {
    fn new(num_threads: usize) -> Self {

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
        
        // Create a pool of threads that are receiving the tasks
        let mut handles: Vec<JoinHandle<()>> = Vec::new();
        for _ in 0..num_threads {

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

        Self {
            handles, 
            sender
        }
    }

    fn submit(&self, task: Task) {
        let _res = self.sender.send(task).unwrap();
    }

    fn join(self) {
        drop(self.sender);
        for h in self.handles {
            h.join().unwrap();
        }
    }
}


fn main() {

    let pool = ThreadPool::new(4);

    for i in 1..100 {
        pool.submit(Box::new(move || f(i)));
    }    

    pool.join();
}