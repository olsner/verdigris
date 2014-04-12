	.text
	.section	.text.memset,"ax",@progbits
	.globl	memset
	.type	memset,@function
memset:
	// rdi = target, rsi = data, rdx = count
	movl %esi, %eax
	movq %rdx, %rcx ; save one byte: assume count < 4GB
	rep stosb
	retq
.Le:
	.size	memset, .Le - memset

