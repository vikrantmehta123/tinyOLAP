use std::{
    sync::{
        Arc,
        atomic::{AtomicI32, Ordering::Relaxed},
    },
    thread,
};

/// The atomic increment function generates assembly code as follows:
/// atomic_increment:
/// 	mov	eax, 1 ; (1) xadd adds a register. So first, 1 needs to be put in a register
/// 	lock xadd dword ptr [rdi], eax ; (2)
/// 	ret; (3)
///
/// In the (2) instruction above, there's two things we need to unpack:
///     lock: This instruction ensures that across cores, the location in 'rdi' register
///         is locked. I believe the cache line for it is locked and no core can access it.
///         I am not yet fully sure of how the hardware implements this lock given that the
///         memory will have L1, L2, L3 caches, etc. But for the moment, let's assume that
///         the hardware provides this guarantee: If an address is locked, then no other core
///         can access this memory location. So the lock instruction, we can assume, requires
///         a memory address as a parameter. 'rdi' contains the address of the location.
///         Technically, "lock" is not an instruction. It's a prefix to an instruction. I
///         think we can specify 1 byte as "prefix" for the instructions we issue to CPU.
///         Only a selected set of instructions can be prefixed.
///         Locking the cache line is NOT one cycle! It takes time because the CPU has
///         to drain out store buffers and lock the cache lines. So even if it is a single
///         instruction in the assembly, it doesn't mean that it executes on the CPU in a
///         single cycle.
///         
///     xadd: Add the delta from 'eax' to the memory location in rdi register, and put the
///         old value back in 'eax' register.
#[unsafe(no_mangle)]
#[inline(never)]
fn atomic_increment(num: &AtomicI32) -> i32 {
    // Oredering::Relaxed is out of scope for this file. We can look at it later.
    num.fetch_add(1, Relaxed)
}

/// The plain increment function generates assembly code as follows:
/// plain_increment:
///	    mov	eax, dword ptr [rdi] ; go to address at rdi and load whatever's there in 'eax'
///	    inc	eax
///	    mov	dword ptr [rdi], eax; store 'eax' back to memory location present at [rdi]
///	    ret
#[inline(never)]
#[unsafe(no_mangle)]
fn plain_increment(num: &mut i32) -> i32 {
    *num += 1;
    *num
}


/// Use the atomic_increment to safely increment the variable
fn atomic_run() {
    let counter = Arc::new(AtomicI32::new(0));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let c = Arc::clone(&counter);
        let h = thread::spawn(move || {
            for _ in 0..100000 {
                atomic_increment(&c);
            }
        });

        handles.push(h);
    }

    for h in handles {
        h.join().unwrap();
    }

    println!("Atomic final = {}", counter.load(Relaxed));
}


/// Borrow checker doesn't let us write unsafe code.
/// So we had to introduce the AtomicI32 for plain increment.
/// 
/// Even though the counter is atomic, the read and write operations
/// to it are separate. They can be non-atomic.
fn plain_run() {
    let counter = Arc::new(AtomicI32::new(0));
    let mut handles = Vec::new();

    for _ in 0..8 {
        let c = Arc::clone(&counter);
        let h = thread::spawn(move || {
            for _ in 0..100_000 {
                // workaround to simulate unsafe code
                // We deliberately split add into load and store.
                // Updates get lost in between the two ops.
                let v = c.load(Relaxed);
                c.store(v + 1, Relaxed);
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().unwrap();
    }

    println!("plain final = {}", counter.load(Relaxed));
}

fn main() {
    // Prints 8 * 100_000
    atomic_run();

    // Prints a val less than 8 * 100_000
    // because the updates are lost
    plain_run();
}
