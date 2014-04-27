	.text
	.section	.text.syscall_entry_stub,"ax",@progbits
	.globl	syscall_entry_stub
	.type	syscall_entry_stub,@function
syscall_entry_stub:
	swapgs
	xchgq %gs:8, %rsp
	# * Save registers that aren't caller-save
	#   That is: rbp, rbx, r12-r15
	# * Save rip and rflags
	# * Fix up for syscall vs normal calling convention.
	#   r10 (caller-save) is used instead of rcx for argument 4
	# * Put the saved registers somewhere nice so the kernel code can put them
	#   in the right place in the process' structure.

	# Arguments are pushed in right-to-left order (in other words, the last
	# to be pushed is the first thing on the stack after the function's return
	# address).
	pushq %r15
	pushq %r14
	pushq %r13
	pushq %r12
	pushq %rbx
	pushq %rbp

	pushq %r11
	pushq %rcx
	movq %r10, %rcx

	# rax has the syscall number, move to a parameter register and move the
	# original r9 to the stack (we usually won't need it though).
	pushq %r9
	movq %rax, %r9

	# The syscall function's prototype is:
	# fn(rdi,rsi,rdx,r10,r8,r9,  rip, rflags,  saved_rbp, saved_rbx, saved_r12, ...)

	.extern syscall
	callq syscall
	# If we return, fall-through to the invalid instruction below

	.globl	syscall_entry_compat
	.type	syscall_entry_compat,@function
syscall_entry_compat:
	# Fail
	ud2
1:
	.size	syscall_entry_compat, 1b - syscall_entry_compat
	.size	syscall_entry_stub, 1b - syscall_entry_stub
