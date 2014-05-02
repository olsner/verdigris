; vim:filetype=nasm:

bits 64

; callee-save: rbp, rbx, r12-r15
; caller-save: rax, rcx, rdx, rsi, rdi, r8-r11
%macro clear_clobbered_syscall 0
	; rax, rcx, r11 are also in this list, but are used for return, rip and rflags respectively.
%endmacro
%macro clear_clobbered 0
	clear_clobbered_syscall
	zero	ecx
	zero	r11
%endmacro

%macro zero 1
	xor	%1, %1
%endmacro

%macro gfunc 1
%%end: global %1:function (%%end - %1)
%endmacro

%assign i 0

%macro	reglabels 1-*
%rep	%0
	.r %+ %1 equ .regs+(i * 8)
	%assign i i+1
	%rotate 1
%endrep
%endmacro

struc	proc
	.regs	resq 16 ; a,c,d,b,sp,bp,si,di,r8-15

	; Aliases for offsets into regs
	reglabels ax,cx,dx,bx,sp,bp,si,di
%rep 8
	reglabels i
%endrep

	.rip	resq 1
	.rflags	resq 1

	.cr3	resq 1

endstruc

%macro load_regs 1-*
%rep %0
%ifidni %1,rdi
	%error rdi is in use by this macro
%else
	mov	%1,[rdi+proc. %+ %1]
%endif
	%rotate 1
%endrep
%endmacro

section .text.fastret, exec
fastret:
	swapgs
	zero	edx
	zero	r8
	zero	r9
	zero	r10
.no_clear:
	mov	rbx, cr3
	cmp	rbx, [rdi + proc.cr3]
	jne	.wrong_cr3
	load_regs rbp,rbx,r12,r13,r14,r15
.fast_fastret:
	mov	rsp, [rdi+proc.rsp]
	mov	rcx, [rdi+proc.rip]
	mov	r11, [rdi+proc.rflags]
	mov	rax, rsi
	o64 sysret
.wrong_cr3:
	ud2
.end:
global	fastret:function (fastret.end - fastret)

section .text.slowret, exec
slowret:
	; TODO
	ud2
.end
global	slowret:function (slowret.end - slowret)

section .text.syscall_entry_stub, exec
syscall_entry_stub:
	swapgs
	; FIXME We have clobberable registers here, use them
	xchg [gs:8], rsp
	; * Save registers that aren't caller-save
	;   That is: rbp, rbx, r12-r15
	; * Save rip and rflags
	; * Fix up for syscall vs normal calling convention.
	;   r10 (caller-save) is used instead of rcx for argument 4
	; * Put the saved registers somewhere nice so the kernel code can put them
	;   in the right place in the process' structure.

	; Arguments are pushed in right-to-left order (in other words, the last
	; to be pushed is the first thing on the stack after the function's return
	; address).
	push	r15
	push	r14
	push	r13
	push	r12
	push	rbx
	push	rbp

	push	r11
	push	rcx
	mov	rcx, r10

	; rax has the syscall number, move to a parameter register and move the
	; original r9 to the stack (we usually won't need it though).
	push	r9
	mov	r9, rax

	; The syscall function's prototype is:
	; fn(rdi,rsi,rdx,r10,r8,r9,  rip, rflags,  saved_rbp, saved_rbx, saved_r12, ...)

	extern syscall
	call syscall
	; If we return, fall-through to the invalid instruction below

syscall_entry_compat:
	; Fail
	ud2

.end
global	syscall_entry_compat:function (syscall_entry_compat.end - syscall_entry_compat)
global	syscall_entry_stub:function (syscall_entry_compat.end - syscall_entry_stub)


section .text.handle_irq_generic, exec

%macro stub 1
	push	byte %1
	jmp	handle_irq_generic
%endmacro

%macro handle_irqN_generic 1
handle_irq_ %+ %1:
	stub %1
%endmacro

irq_handlers:

%assign irq 32
%rep 17
handle_irqN_generic irq
%assign irq irq + 1
%endrep

gfunc irq_handlers

handle_irq_generic:
	ud2
gfunc handle_irq_generic

handler_NM_stub stub 7
gfunc handler_NM_stub
handler_PF_stub stub 14
gfunc handler_PF_stub

