/// Loop Carried Dependence
/// 'pref_array' is the prefix sum array
/// 'nums' is the array that is being prefix-summed
/// 
/// This is terrible for loop vectorization. By vectorization, we mean
/// that instead of iterating one item at a time, we iterate
/// eight items at a time. This can only be done if the current
/// value doesn't depend on the previous value(s). In this loop, 
/// the current val depends on the previous one. Hence, the
/// compiler can't vectorize.
/// 
/// There is an algorithm that makes prefix sum SIMD friendly.
/// But it might not be straightforward to implement. So we don't.
/// Regardless, this illustrates why dependence is bad.
#[unsafe(no_mangle)]
#[inline(never)]
fn bad_prefix_sum(pref_array: &mut [i32], nums : &[i32]) {
    for i in 1..pref_array.len() {
        pref_array[i] = pref_array[i-1] + nums[i];
    }
}


/// This is bad loop boundary. Compiler cannot know whether
/// whether idx[i] is in bounds or not. So the compiler has to 
/// introduce this:
///     if idx[i] >= arr.len() { panic_bounds_check(); }
/// 
/// In each iteration, the compiler has to have the above check.
/// So the compiler cannot vectorize this.
/// Instead, we see a single register loop with jump instructions.
/// .LBB5_2:
///    movq  (%rdx,%r9,8), %r8    ; r8 = idx[i]              (load the index)
///    cmpq  %rsi, %r8            ; idx[i]  vs  arr.len()    BOUNDS CHECK,
///    jae   .LBB5_5             ;   if idx[i] >= len → panic every iteration
///    incq  %r9                  ; i++
///    addl  (%rdi,%r8,4), %eax   ; total += arr[idx[i]]     (SCALAR load + add)
///    cmpq  %r9, %rcx
///    jne   .LBB5_2
/// 
/// There may be some cases where the compiler can hoist out the
/// panic check. For example, instead of an idx slice, you have a 
/// parameter/constant called 'n' and you iterate over 0..n.
/// In this case, compiler can hoist the panic_check out.
/// Regardless, ideally, you would want to iterate over the 
/// arr.len() or use the iterator.
#[unsafe(no_mangle)]
#[inline(never)]
fn bad_bounding_strategy(arr: &[i32], idx: &[usize]) -> i32 {

    let mut total = 0;
    for i in 0..idx.len() {
        total += arr[idx[i]];
    }

    total
}

/// For floats, some reductions cannot be vectorized. 
/// Because due to precision errors some float reductions are 
/// not associative.
/// For example: (a+b)+c != a+(b+c) for floats due to weird 
/// precision errors.
/// 
/// Vectorizing reductions requires reordering of the reduction 
/// operation and because it's not associative, reduction may 
/// is not possible on floats by default.
/// 
/// But note that if you have element wise float operations, 
/// like: a[i] * 2.0, those can still be vectorized.
/// 
/// For many cases, we can work without the last few precision points.
/// So we need to explicitly tell the compiler that it's safe to 
/// reorder the reduction on floats.
/// 
/// By default, sum, product are not vectorized for floats.
/// Since sum is not vectorized, avg also isn't vectorized.
/// But min, max, count, and, or, xor are vectorized for floats.
/// 
/// In C/C++, we have ffast-math flag that tells compiler to assume
/// that floats are associative on sum/product and then we can 
/// vectorize. In rust, we don't have such flag.
#[unsafe(no_mangle)]
#[inline(never)]
fn sum_f32(a: &[f32]) -> f32 {
    let mut s = 0.0;
    for &x in a { s += x; }
    s
}

fn main() {
    
}