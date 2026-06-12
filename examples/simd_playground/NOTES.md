# Background Information on Compiler Optimization

`rustc` by itself does no vectorization at all. It simply lowers the code into the LLVM IR. Then LLVM has two optimization passes that do the vectorization:
- Loop Vectorizer: Modify the loop to operate on 4 or 8 items at a time, instead of one at a time.
- SLP Vectorizer: Finds "straight line" code (the code that is not in loops) but that does same operation on adjacent memory.

The whole goal is: to mathematically prove that the batching/reordering of the instructions does not change the result. If it doesn't then the compiler will try to vectorize. This proof is done by using the memory dependence graph- what memory locations are accessed in which iterations.

There are some earlier passes that do some other optimizations.

## Inlining

Look at the following code:

```rust
fn double(x: i32) -> i32 { x * 2 }

for i in 0..n {
    dst[i] = double(src[i]);
}
```

For the compiler to be able to vectorize the loop, it needs to know what the double function does. Otherwise it can't make the assumptions about memory locality, independence, etc.

So compilers often do inlining, where they replace the call to double inside the loop with the actual function body. From compiler point of view, it's just changing the child subtree for the loop node in the IR.

When we chain functions in Rust, the compiler is able to inline them well. That's why we prefer chaining functions in idiomatic Rust.

Often, inlining is driven by some cost heuristics. So we can't guarantee what will be inlined and what won't be.

## SROA (Scalar Replacement of Aggregates)

Imagine we are writing a compiler for Rust. The user can write structs or arrays or other composite types in their code.

But when generating instructions, we can't say: place instance of struct X on to a register. 

We need to break the struct down into scalar values like ints and floats.

To do this, we introduce these notions of virtual registers and Single Static Assignment (SSA).

As a compiler, in the early passes we assume that we have infinite hardware registers. So we can simply break up the individual elements of the struct and assume we place them on a separate virtual register. 

**Note:** SROA also promotes scalars that live on stack memory into virtual registers. Not fully sure here as well, as to why.

Further, we make an assumption that every register will get allocated a value only once. For example, the following code:

```
x = 5
if something {
    x = x + 1;
}
else {
    x = x + 2;
}
print(x);
```

This gets SSA'ed as:

```
x_1 = 5
if something {
    x_2 = x_1 + 1;
}
else {
    x_3 = x_1 + 2;
}

x_4 = phi(x_3, x_2);
print(x_4);
```

Doing these things allow for a lot more optimizations later. So almost all compilers will do this. I am not fully sure how this unlocks the optimizations, including vectorization, downstream but let's assume it does.

This SROA pass runs early in the optimization so that later optimizations can work with it. 

Finally, there is another pass at the end, that maps back the virtual registers to the physical ones.

## InstCombine and SimplifyCFG

These are cleanup passes that run almost after every other optimization pass. They eliminate dead code or normalize code for subsequent passes.

The later passes like SCEV operate on pattern matching. So these two optimization passes try to normalize the code so that it is in the canonical form.

## LICM (Loop Invariant Code Motion)

Analyse the loop body and move the code that isn't required in the loop out of the loop. Again, LLVM needs to mathematically prove that the body we're taking out is not needed in the loop.

If the compiler cannot prove this/is uncertain, it doesn't move it out.

This proof is done by proving that a particular value doesn't change/written to across iterations. Rust's borrow rules also help enforce this.

## LoopSimplify & LoopRotate

Ensure that there is one entry and one exit for the loop, or convert a simple while loop to do-while loop, etc.

The loop vectorizer can vectorize iteration if it can predict for how many iterations the loop will run.

So continue statements are fine. But take a break/return  statement like this:

```rust
if a[i] > 0 { break ;}
```

Here, the compiler cannot guarantee how many iterations will the loop run for because it depends on the data. The data may not be known upfront.

Having multiple break/return statements is even worse.

As a rule of thumb: continue statements are fine. Tight loops are fine. Data dependent, multiple breaks might be bad.

Note that it's not that the break statement will always cause problems. It may or may not depending on how other optimizations go. But a tight loop helps the compiler figure out how many times the iteration will run and then it can vectorize.

So when writing code: write it the obvious way first. But if you observe breaks/returns in the loop, try to see if you can restructure the code.

## IndVarSimplify & SCEV (ScalarEvolution)

Loops have induction variables- the variables that change with every iteration. For example, the loop counter. At times, you have other vars also that change with the loop (in each iteration). Induction Variable Simplify canonicalises the induction variables so that SCEV can optimize them.

An induction variable that "occasionally" changes or depends on data, makes it impossible for IndVarSimplify to canonicalise. So wherever possible, avoid them. Use simple loops or iterators.

The whole reason we do this is to let SCEV compute the number of times the loop will run mathematically. We can do some simple algebra (solving for recurrence) to arrive at the counter of how many times the loop will run. Then SCEV knows- I can vectorize n/8 times and then I have to do last n%8 elements.

The compiler can then vectorize the loop.

But note that we have two things influencing this vectorization at SCEV level.

1. When the loop will end should be knowable.
2. The loop body must be vectorizable => Each memory access also should be vectorizable here. 

For example, consider the following loop:

```rust
let mut j = 0;
for i in 0..n {
    if a[i] > 0 {
        out[j] = i;       // j is used here. This is problem
        j += 1;          // ← j only sometimes advances
    }
}
```

In this case, the compiler knows that the outer loop will run n times. So it can precompute `i` to vectorize it. But it cannot precompute `j`! That's not a vectorizable operation.

A recurrence requires constant step in each iteration. So if your induction variables have recurrence, you're good. Else you're not.

---

# SIMD Playground

The vectorizer thinks in dataflow graphs, not "operations".

If the compiler can predict the number of times the loop will run and if every operation in the loop has a corresponding vectorized form and if the memory addresses predictable change in each iteration, then compiler will try to vectorize it.

There are vectorized compare instructions as well that avoid branching the code and instead create masks. So without jump instructions, the compiler can filter and then select in a vectorized manner using this mask.

The compiler tries to first prove whether it is mathematically correct to run the loop 8 elems at a time. This is done by the memory dependence graph. If no dependency on previous iteration, one check is passed.

There's also a cost model that I am ignoring here, because we assume that we will be processing, say, millions of rows.

**Note:** f32/f64 sum is not associative! Compiler will refuse to reduce it and vectorize it.

## Basics of Registers and Assembly Language

Computers have traditional registers that are 32 or 64 bit wide. These are scalar registers- they hold one value. For example, `rax` register can hold on 64-bit value. There are instructions

But modern computer hardware supports much wider registers as well. For example, 128 bit or 256 bit or even 512 bits. These are vector registers.
- xmm0: 128-bit
- ymm0: 256-bit
- zmm0: 512-bit

Similar to scalar registers, if your machine has 256-bit registers, then their lower 128-bits are used as `xmm` registers.

We can fit more than one value in these registers. Logically, one wide register can be partitioned into several lanes. For example, we can fit eight 32-bit integers in a `ymm` register.

The packed values in a vector register are called lanes. A 256-bit ymm register holds:

ymm0:  [ f32 | f32 | f32 | f32 | f32 | f32 | f32 | f32 ]
        lane0 lane1 lane2 lane3 lane4 lane5 lane6 lane7

Smaller datatypes can be vectorized harder.

Correspondingly, there are vectorized instructions also that operate on these wide registers and on those lanes.

```s
; SCALAR — one add, one result
addss  xmm0, xmm1        ; xmm0[0] = xmm0[0] + xmm1[0]   (lowest lane only)

; PACKED — eight adds, one instruction
vaddps ymm0, ymm0, ymm1  ; for each lane k: ymm0[k] = ymm0[k] + ymm1[k]
```

The prefix `v` denotes vectorized. The suffix `ps` also holds significance. Look up docs.

Also, note that the CPU cares nothing about how the memory is accessed. Of course, it has latency effects, but from a CPU instruction point of view, there is no distinction between a memory access from L1 cache or from the RAM.