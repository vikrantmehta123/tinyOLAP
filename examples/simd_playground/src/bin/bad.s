	.file	"bad.efb6a02c1cfe3d11-cgu.0"
	.section	.text._ZN3bad4main17h1e2608e1169d2be6E,"ax",@progbits
	.hidden	_ZN3bad4main17h1e2608e1169d2be6E
	.globl	_ZN3bad4main17h1e2608e1169d2be6E
	.p2align	4
	.type	_ZN3bad4main17h1e2608e1169d2be6E,@function
_ZN3bad4main17h1e2608e1169d2be6E:
	.cfi_startproc
	retq
.Lfunc_end0:
	.size	_ZN3bad4main17h1e2608e1169d2be6E, .Lfunc_end0-_ZN3bad4main17h1e2608e1169d2be6E
	.cfi_endproc

	.section	.text._ZN3std2rt10lang_start17h575ca2c9d98f2372E,"ax",@progbits
	.hidden	_ZN3std2rt10lang_start17h575ca2c9d98f2372E
	.globl	_ZN3std2rt10lang_start17h575ca2c9d98f2372E
	.p2align	4
	.type	_ZN3std2rt10lang_start17h575ca2c9d98f2372E,@function
_ZN3std2rt10lang_start17h575ca2c9d98f2372E:
	.cfi_startproc
	pushq	%rax
	.cfi_def_cfa_offset 16
	movl	%ecx, %r8d
	movq	%rdx, %rcx
	movq	%rsi, %rdx
	movq	%rdi, (%rsp)
	leaq	.Lanon.03522cf0cb9adbe5a325668e031ea2b6.0(%rip), %rsi
	movq	%rsp, %rdi
	callq	*_ZN3std2rt19lang_start_internal17hc68d929ebd5f7eeaE@GOTPCREL(%rip)
	popq	%rcx
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end1:
	.size	_ZN3std2rt10lang_start17h575ca2c9d98f2372E, .Lfunc_end1-_ZN3std2rt10lang_start17h575ca2c9d98f2372E
	.cfi_endproc

	.section	".text._ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17hb73b2587900a95f2E","ax",@progbits
	.p2align	4
	.type	_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17hb73b2587900a95f2E,@function
_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17hb73b2587900a95f2E:
	.cfi_startproc
	pushq	%rax
	.cfi_def_cfa_offset 16
	movq	(%rdi), %rdi
	callq	_ZN3std3sys9backtrace28__rust_begin_short_backtrace17hbaabfd9da35195d4E
	xorl	%eax, %eax
	popq	%rcx
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end2:
	.size	_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17hb73b2587900a95f2E, .Lfunc_end2-_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17hb73b2587900a95f2E
	.cfi_endproc

	.section	.text._ZN3std3sys9backtrace28__rust_begin_short_backtrace17hbaabfd9da35195d4E,"ax",@progbits
	.p2align	4
	.type	_ZN3std3sys9backtrace28__rust_begin_short_backtrace17hbaabfd9da35195d4E,@function
_ZN3std3sys9backtrace28__rust_begin_short_backtrace17hbaabfd9da35195d4E:
	.cfi_startproc
	pushq	%rax
	.cfi_def_cfa_offset 16
	callq	*%rdi
	#APP
	#NO_APP
	popq	%rax
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end3:
	.size	_ZN3std3sys9backtrace28__rust_begin_short_backtrace17hbaabfd9da35195d4E, .Lfunc_end3-_ZN3std3sys9backtrace28__rust_begin_short_backtrace17hbaabfd9da35195d4E
	.cfi_endproc

	.section	".text._ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17hd3d661e9699afbbdE","ax",@progbits
	.p2align	4
	.type	_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17hd3d661e9699afbbdE,@function
_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17hd3d661e9699afbbdE:
	.cfi_startproc
	pushq	%rax
	.cfi_def_cfa_offset 16
	movq	(%rdi), %rdi
	callq	_ZN3std3sys9backtrace28__rust_begin_short_backtrace17hbaabfd9da35195d4E
	xorl	%eax, %eax
	popq	%rcx
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end4:
	.size	_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17hd3d661e9699afbbdE, .Lfunc_end4-_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17hd3d661e9699afbbdE
	.cfi_endproc

	.section	.text.sum_f32,"ax",@progbits
	.globl	sum_f32
	.p2align	4
	.type	sum_f32,@function
sum_f32:
	.cfi_startproc
	testq	%rsi, %rsi
	je	.LBB5_1
	leaq	(,%rsi,4), %rcx
	addq	$-4, %rcx
	movl	%ecx, %eax
	notl	%eax
	testb	$28, %al
	jne	.LBB5_5
	vxorps	%xmm0, %xmm0, %xmm0
	movq	%rdi, %rax
	jmp	.LBB5_7
.LBB5_1:
	vxorps	%xmm0, %xmm0, %xmm0
	retq
.LBB5_5:
	movl	%ecx, %edx
	shrl	$2, %edx
	incl	%edx
	andl	$7, %edx
	negq	%rdx
	vxorps	%xmm0, %xmm0, %xmm0
	movq	%rdi, %rax
	.p2align	4
.LBB5_6:
	vaddss	(%rax), %xmm0, %xmm0
	addq	$4, %rax
	incq	%rdx
	jne	.LBB5_6
.LBB5_7:
	cmpq	$28, %rcx
	jb	.LBB5_2
	leaq	(%rdi,%rsi,4), %rcx
	.p2align	4
.LBB5_9:
	vaddss	(%rax), %xmm0, %xmm0
	vaddss	4(%rax), %xmm0, %xmm0
	vaddss	8(%rax), %xmm0, %xmm0
	vaddss	12(%rax), %xmm0, %xmm0
	vaddss	16(%rax), %xmm0, %xmm0
	vaddss	20(%rax), %xmm0, %xmm0
	vaddss	24(%rax), %xmm0, %xmm0
	vaddss	28(%rax), %xmm0, %xmm0
	addq	$32, %rax
	cmpq	%rcx, %rax
	jne	.LBB5_9
.LBB5_2:
	retq
.Lfunc_end5:
	.size	sum_f32, .Lfunc_end5-sum_f32
	.cfi_endproc

	.section	.text.sum_i32,"ax",@progbits
	.globl	sum_i32
	.p2align	4
	.type	sum_i32,@function
sum_i32:
	.cfi_startproc
	testq	%rsi, %rsi
	je	.LBB6_1
	leaq	(,%rsi,4), %r8
	addq	$-4, %r8
	xorl	%eax, %eax
	movq	%rdi, %r9
	cmpq	$12, %r8
	jb	.LBB6_14
	movq	%r8, %rcx
	shrq	$2, %rcx
	incq	%rcx
	movabsq	$9223372036854775776, %rdx
	cmpq	$124, %r8
	jae	.LBB6_9
	xorl	%r8d, %r8d
	xorl	%eax, %eax
	jmp	.LBB6_5
.LBB6_1:
	xorl	%eax, %eax
	retq
.LBB6_9:
	movq	%rcx, %r8
	andq	%rdx, %r8
	vpxor	%xmm0, %xmm0, %xmm0
	xorl	%eax, %eax
	vpxor	%xmm1, %xmm1, %xmm1
	vpxor	%xmm2, %xmm2, %xmm2
	vpxor	%xmm3, %xmm3, %xmm3
	.p2align	4
.LBB6_10:
	vpaddd	(%rdi,%rax,4), %ymm0, %ymm0
	vpaddd	32(%rdi,%rax,4), %ymm1, %ymm1
	vpaddd	64(%rdi,%rax,4), %ymm2, %ymm2
	vpaddd	96(%rdi,%rax,4), %ymm3, %ymm3
	addq	$32, %rax
	cmpq	%rax, %r8
	jne	.LBB6_10
	vpaddd	%ymm0, %ymm1, %ymm0
	vpaddd	%ymm2, %ymm3, %ymm1
	vpaddd	%ymm0, %ymm1, %ymm0
	vextracti128	$1, %ymm0, %xmm1
	vpaddd	%xmm1, %xmm0, %xmm0
	vpshufd	$238, %xmm0, %xmm1
	vpaddd	%xmm1, %xmm0, %xmm0
	vpshufd	$85, %xmm0, %xmm1
	vpaddd	%xmm1, %xmm0, %xmm0
	vmovd	%xmm0, %eax
	cmpq	%r8, %rcx
	je	.LBB6_8
	testb	$28, %cl
	je	.LBB6_13
.LBB6_5:
	addq	$28, %rdx
	andq	%rcx, %rdx
	leaq	(%rdi,%rdx,4), %r9
	vmovd	%eax, %xmm0
	.p2align	4
.LBB6_6:
	vpaddd	(%rdi,%r8,4), %xmm0, %xmm0
	addq	$4, %r8
	cmpq	%r8, %rdx
	jne	.LBB6_6
	vpshufd	$238, %xmm0, %xmm1
	vpaddd	%xmm1, %xmm0, %xmm0
	vpshufd	$85, %xmm0, %xmm1
	vpaddd	%xmm1, %xmm0, %xmm0
	vmovd	%xmm0, %eax
	cmpq	%rdx, %rcx
	jne	.LBB6_14
	jmp	.LBB6_8
.LBB6_13:
	leaq	(%rdi,%r8,4), %r9
.LBB6_14:
	leaq	(%rdi,%rsi,4), %rcx
	.p2align	4
.LBB6_15:
	addl	(%r9), %eax
	addq	$4, %r9
	cmpq	%rcx, %r9
	jne	.LBB6_15
.LBB6_8:
	vzeroupper
	retq
.Lfunc_end6:
	.size	sum_i32, .Lfunc_end6-sum_i32
	.cfi_endproc

	.section	.text.main,"ax",@progbits
	.globl	main
	.p2align	4
	.type	main,@function
main:
	.cfi_startproc
	pushq	%rax
	.cfi_def_cfa_offset 16
	movq	%rsi, %rcx
	movslq	%edi, %rdx
	leaq	_ZN3bad4main17h1e2608e1169d2be6E(%rip), %rax
	movq	%rax, (%rsp)
	leaq	.Lanon.03522cf0cb9adbe5a325668e031ea2b6.0(%rip), %rsi
	movq	%rsp, %rdi
	xorl	%r8d, %r8d
	callq	*_ZN3std2rt19lang_start_internal17hc68d929ebd5f7eeaE@GOTPCREL(%rip)
	popq	%rcx
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end7:
	.size	main, .Lfunc_end7-main
	.cfi_endproc

	.type	.Lanon.03522cf0cb9adbe5a325668e031ea2b6.0,@object
	.section	.data.rel.ro..Lanon.03522cf0cb9adbe5a325668e031ea2b6.0,"aw",@progbits
	.p2align	3, 0x0
.Lanon.03522cf0cb9adbe5a325668e031ea2b6.0:
	.asciz	"\000\000\000\000\000\000\000\000\b\000\000\000\000\000\000\000\b\000\000\000\000\000\000"
	.quad	_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17hd3d661e9699afbbdE
	.quad	_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17hb73b2587900a95f2E
	.quad	_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17hb73b2587900a95f2E
	.size	.Lanon.03522cf0cb9adbe5a325668e031ea2b6.0, 48

	.ident	"rustc version 1.94.1 (e408947bf 2026-03-25)"
	.section	".note.GNU-stack","",@progbits
