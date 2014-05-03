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

%macro restruc 1-2 1
	resb (%1 %+ _size) * %2
%endmacro

%assign i 0

%macro	reglabels 1-*
%rep	%0
	.r %+ %1 equ .regs+(i * 8)
	%assign i i+1
	%rotate 1
%endrep
%endmacro

struc iframe
	.rip	resq 1
	.cs	resq 1
	.rflags	resq 1
	.rsp	resq 1
	.ss	resq 1
endstruc

struc gseg
	.self	resq 1
	.rsp	resq 1
	.proc	resq 1
endstruc

struc	proc, -0x80
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
%define %%reg %1
%rotate 1
%rep (%0 - 1)
%ifidni %1,%%reg
	%error %%reg is in use by this macro
%else
	mov	%1,[%%reg+proc. %+ %1]
%endif
	%rotate 1
%endrep
%endmacro

%macro save_regs 1-*
%define %%reg %1
%rotate 1
%rep (%0 - 1)
%ifidni %1,%%reg
	%error %%reg is in use by this macro
%else
	mov	[%%reg+proc. %+ %1], %1
%endif
	%rotate 1
%endrep
%endmacro

%macro gfunc 1
%%end: global %1:function (%%end - %1)
%endmacro

%macro proc 1-2 1
%ifnidn %2,NOSECTION
section .text.%1, exec
%endif
%push proc
%1:
%define %$LAST_PROC %1
%endmacro

%macro endproc 0
%ifnctx proc
	%error unmatched endproc
%endif
gfunc %$LAST_PROC
; %%end:
; global %$LAST_PROC:function (%%end - %$LAST_PROC)
%pop
%endmacro

proc fastret
	swapgs
	zero	edx
	zero	r8
	zero	r9
	zero	r10
.no_clear:
	sub	rdi, proc
	mov	rbx, cr3
	cmp	rbx, [rdi + proc.cr3]
	jne	.wrong_cr3
	load_regs rdi,  rbp,rbx,r12,r13,r14,r15
.fast_fastret:
	mov	rsp, [rdi + proc.rsp]
	mov	rcx, [rdi + proc.rip]
	mov	r11, [rdi + proc.rflags]
	mov	rax, rsi
	o64 sysret
.wrong_cr3:
	ud2
endproc

; section .text.syscall_entry_stub, exec
; syscall_entry_stub:
proc syscall_entry_stub
	swapgs
	; FIXME We have clobberable registers here, use them
	xchg [gs:8], rsp
	; TODO Store in process instead.
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

proc syscall_entry_compat, NOSECTION
	; Fail
	ud2
endproc

; .end
; global	syscall_entry_compat:function (syscall_entry_compat.end - syscall_entry_compat)
;global	syscall_entry_stub:function (syscall_entry_compat.end - syscall_entry_stub)
endproc


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

%macro combine 1-*
 %assign i 0
 %rotate 1
 %rep (%0 - 1)
  %assign i i | (1 << %1)
  %rotate 1
 %endrep
 %1 EQU i
%endmacro

combine EXC_ERR_MASK, 8, 10, 11, 12, 13, 14, 17

%macro cond 2
	j%-1	%%skip
	%2
%%skip:
%endmacro

; Stack when we get here (from low to high address)
; vector
; (error)
; rip
; cs
; rflags
; rsp
; ss
; some tasks:
; get gseg
; save rip, rflags, rsp to process
; save all caller-save regs to process
proc handle_irq_generic, NOSECTION
	push	rax
	zero	eax
	swapgs
	mov	rax, [gs:rax + gseg.proc]
	pop	qword [rax - proc + proc.rax] ; The rax saved on entry
	sub	rax, proc
	mov	[rax + proc.rdx], rdx
	mov	[rax + proc.rdi], rdi
	mov	[rax + proc.rsi], rsi
	pop	rdi ; vector
	zero	esi ; error (0 if no code for exception)

	cmp	edi, byte 32
	jae	.no_err
	mov	edx, EXC_ERR_MASK
	bt	edx, edi
	cond c, pop rsi
.no_err:

	; could use a register, but that's boring (and larger code)
%macro save_iframe 1-*
%rep %0
	%rotate 1
	push	qword [rsp + iframe. %+ %1]
	pop	qword [rax + proc. %+ %1]
%endrep
%endmacro
	save_iframe rip, rflags, rsp

	; already saved: ax, dx, di, si
	; caller-save: rax, rcx, rdx, rsi, rdi, r8-r11
	; calee-save regs are not saved here because we assume that the
	; compiler generated irq_entry code is correct.
	save_regs rax,  rcx,r8,r9,r10,r11

	; Set flags to a known state.
	push	byte 0
	popfq

	; Now rdi = vector, rsi = error (or 0)
	extern	irq_entry
	call	irq_entry

	; Fallthrough: implement a slightly more efficient return from
	; interrupt based on the subset of regs that are preserved by calling
	; convention.

	zero	edi
	mov	rdi, [gs:rdi + gseg.proc]
	sub	rdi, proc ; offset because it comes from Rust code
	jmp	slowret.from_int

; slowret: all registers are currently unknown, load *everything* from process
; (in rdi), then iretq
proc slowret, NOSECTION
	sub	rdi, proc ; offset because it comes from Rust code
	; All the callee-saves
	load_regs rdi,  rbp,rbx,r12,r13,r14,r15
.from_int:
	; all caller-saves except rdi
	load_regs rdi,  rax,rcx,rsi,rdx,r8,r9,r10,r11
	; Push stuff for iretq
user_code_seg	equ	56
user_cs		equ	user_code_seg+16 | 11b
user_ds		equ	user_cs+8
	push	user_ds
	push	qword [rdi + proc.rsp]
	push	qword [rdi + proc.rflags]
	push	user_cs
	push	qword [rdi + proc.rip]
	mov	rdi, [rdi + proc.rdi]
	swapgs
	iretq
endproc

endproc

handler_NM_stub stub 7
gfunc handler_NM_stub
handler_PF_stub stub 14
gfunc handler_PF_stub

