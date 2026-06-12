#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

/// Often times, to get rid of loop carried dependence, we
/// need to change the algorithm altogether. There may be SIMD
/// friendly algorithms for most use cases. They might be a bit
/// more work but they are there. For example, here, we have a
/// SIMD friendly prefix sum algorithm.
///
/// It works by using Shift-and-Add strategy.
/// For simplicity, let's assume we have a vector register that
/// can pack 4 elements in it. For example, four 32-bit integers.
/// Let the array be:
///     nums = [2, 1, 3, 5, 1, 4, 2, 3, 6, 2]
/// We process the array in chunks of four. For each chunk,
/// we perform two phases:
/// Phase 1:
///     Intra Vector Sum: Perform a parallel shift and add routine
///         this vector chunk.
///         The shift & routine itself has two steps. In first, we
///         shift by one place and add to self. In second, we shift
///         by two place and add to self.
///     Apply Carry: Add the last element of the previous completed
///         chunk to every lane of the current vector
///
/// Let's dry run this:
/// Chunk 1: [2, 1, 3, 5]
/// Phase 1: Shift & Add =>
///     Step 1: [2, 1, 3, 5] + [0, 2, 1, 3] = [2, 3, 4, 8]
///     Step 2: [2, 3, 4, 8] + [0, 0, 2, 3] = [2, 3, 6, 11]
/// Phase 2: Apply Carry => No previous carry is there.
///     so we simply return [2, 3, 6, 11] for this chunk,
///     and the new carry value is 11.
///
/// Chunk 2: [1, 4, 2, 3]
/// Phase 1: Shift & Add =>
///     Step 1: [1, 4, 2, 3] + [0, 1, 4, 2] = [1, 5, 6, 5]
///     Step 2: [1, 5, 6, 5] + [0, 0, 1, 5] = [1, 5, 7, 10]
/// Phase 2: Apply Carry:
///     Previous carry was 11. So we broadcast it and add it:
///         [1, 5, 7, 10] + [11, 11, 11, 11] = [12, 16, 18, 21]
///     For this chunk, we return [12, 16, 18, 21]
///     and the new carry value is 21.
///
/// Chunk 3: [6, 2]
/// Phase 1: Shift & Add
///     Step 1: [6, 2, 0, 0] + [0, 6, 2, 0] = [6, 8, 2, 0]
///     Step 2: [6, 8, 2, 0] + [0, 0, 6, 8] = [6, 8, 8, 8]
/// Phase 2: Apply Carry
///     Previous carry was 21. So we broadcast and add it.
///         [6, 8, 8, 8] + [21, 21, 21, 21] = [27, 29, 29, 29]
/// We need to discard the padded items because only two elems
/// are there. So we return [27, 29] for this chunk.
///
/// Final returned array: [2, 3, 6, 11, 12, 16, 18, 21, 27, 29]
/// 
/// In the code below, we are only working with x86_64 architecture.
/// And we are not worried about integer overflow.
#[cfg(target_arch = "x86_64")]
#[unsafe(no_mangle)]
#[inline(never)]
fn good_prefix_sum(pref_array: &mut [i32], nums: &[i32]) {
    assert!(pref_array.len() >= nums.len(), "pref_array must be at least as long as nums");

    // The carry for the current chunk
    let mut carry = 0i32;

    // What chunk we are at?
    let mut i = 0;


    // SAFETY: assert guarantees pref_array.len() >= nums.len(); the loop
    // condition i+4 <= nums.len() keeps every 16-byte load/store in bounds.
    unsafe {

        while i + 4 <= nums.len() {
            // The si128 is a vector register. We have 32 bit integer array.
            // So with this, we are essentially loading 128 bits from the
            // ptr into the register => Load the four integers in register.
            // This means loading the four integer chunks of the array.
            // The index 'i' gives us the starting index/ptr of the chunk.
            let mut v = _mm_loadu_si128(nums.as_ptr().add(i) as *const __m128i);

            // Phase 1, Step 1. Shift the vector chunk 'v' to the right.
            // We do this by shifting it by 4 bytes.
            let shift_1 = _mm_slli_si128(v, 4);

            // Phase 1, Step 1. Add
            v = _mm_add_epi32(v, shift_1);

            // Phase 1, Step 2: Shift by two numbers i.e. 8 bytes
            let shift_2 = _mm_slli_si128(v, 8);

            // Phase 1, Step 2. Add
            v = _mm_add_epi32(v, shift_2);

            // Phase 2: Broadcast and Apply Carry
            v = _mm_add_epi32(v, _mm_set1_epi32(carry));

            // Store the chunk's prefix sum in the output array. Again we 
            // use 'i' to get the pointer to the start of the output chunk.
            _mm_storeu_si128(pref_array.as_mut_ptr().add(i) as *mut __m128i, v);
            
            carry = pref_array[i + 3];
            i += 4;
        }
    }
    // The remaining n%4 elements to be added in the pref_array
    // We simply loop over them and do scalar addition.
    while i < nums.len() {
        pref_array[i] = carry + nums[i];
        carry = pref_array[i];
        i += 1;
    }

}

/// A sum is a reduction operation. For integer types, reduction
/// operations can be vectorized. It vectorizes because sum
/// is an associative operation. The way it will vectorize it 
/// is as follows:
///     let nums = [1, 2, 3, 4, 5, 6, 7, 8]
///     
///     Step 1: vpaddd: [1, 2, 3, 4] + [5, 6, 7, 8] = [6, 8, 10, 12]
///     Step 2: Either again do vpaddd or do scalar adds on this.
/// 
/// Note that the ordering got changed. It is no longer (1+2+3+...)
/// but rather (1+5) + (2+6)...
/// 
/// By default, every reduction operation can be vectorized for ints.
#[unsafe(no_mangle)]
#[inline(never)]
fn sum_i32(a: &[i32]) -> i32 {
    let mut s = 0;
    for &x in a { s += x; }
    s
}


/// Float sum/product are not vectorizable by default.
/// So we make them vectorizable.
/// 
/// Instead of keeping one accumulator, we keep 4 accumulators.
/// And we load four elements at a time in the 128-bit registers
/// and then we add those into the accumulators.
/// 
/// At the end, we sum these accumulators to get the final result.
/// 
/// We explicitly wrote the code that is vectorized. The compiler
/// isn't vectorizing this. We are.
#[unsafe(no_mangle)]
#[inline(never)]
fn sum_f32(a: &[f32]) -> f32 {
    unsafe {
        // 4 lanes, all initialized to 0.0
        let mut acc = _mm_setzero_ps();  
        
        let mut i = 0;
        while i + 4 <= a.len() {
            let v = _mm_loadu_ps(a.as_ptr().add(i));   // load 4 floats
            acc = _mm_add_ps(acc, v);                  // PACKED add — 4 at once
            i += 4;
        }

        let mut s = 0.0;
        while i < a.len() {
            s += a[i];
            i += 1;
        }

        let mut tmp = [0.0f32; 4];                  // a normal array of 4 floats
        _mm_storeu_ps(tmp.as_mut_ptr(), acc);       // copy acc's 4 lanes → tmp[0..4]
        s += tmp[0] + tmp[1] + tmp[2] + tmp[3];  // now they're plain f32 — just add


        s
    }
}



fn main() {
    let mut out = [0; 10];
    good_prefix_sum(&mut out, &[2, 1, 3, 5, 1, 4, 2, 3, 6, 2]);
    println!("{:?}", out);   // expect [2, 3, 6, 11, 12, 16, 18, 21, 27, 29]
}


