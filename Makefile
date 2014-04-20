# Note: currently works with rustc 0.10 (newer rustc break rust-core)
RUSTC ?= rustc
CLANG ?= clang
CC = clang
LLVM = -3.5
LLVM_LINK = llvm-link$(LLVM)
OPT = opt$(LLVM)
LLVM_DIS = llvm-dis$(LLVM)

TARGET = x86_64-pc-linux-elf
CFLAGS = -std=c99 -fomit-frame-pointer $(COPTFLAGS)
CFLAGS += --target=$(TARGET) -mcmodel=kernel -mno-red-zone
#CFLAGS += -Wa,--no-target-align
LDFLAGS = --check-sections --gc-sections
OPT_LEVEL ?= 2
COPTFLAGS = -Oz -ffunction-sections -fdata-sections
OPTFLAGS = $(COPTFLAGS) -internalize-public-api-list=start64 -internalize
RUSTCFLAGS = --opt-level=$(OPT_LEVEL) --target $(TARGET)

all: rust_kernel rust_kernel.elf

clean:
	rm -fr $(OUTFILES)

KERNEL_OBJS = runtime.o
KERNEL_OBJS += amalgam.o

OUTFILES += $(KERNEL_OBJS)
OUTFILES += main.o rest-core/core.o

rust_kernel: SHELL=/bin/bash
rust_kernel: linker.ld $(KERNEL_OBJS)
	$(LD) $(LDFLAGS) -o $@ -Map $@.map -T $^
	@echo $@: `stat -c%s $@` bytes
	@echo $@: `grep fill $@.map | tr -s ' ' | cut -d' ' -f4 | while read REPLY; do echo $$[$$REPLY]; done | paste -sd+ | bc` bytes wasted on alignment
rust_kernel.elf: linker.ld $(KERNEL_OBJS)
	$(LD) $(LDFLAGS) --oformat=elf64-x86-64 -o $@ -T $^

OUTFILES += rust_kernel rust_kernel.elf rust_kernel.map

main.bc: main.rs rust-core/crate.stamp Makefile
	$(RUSTC) $(RUSTCFLAGS) --crate-type=lib --emit=bc -L. -Lrust-core -o $@ $<

main.bc: mboot.rs start32.rs x86.rs

amalgam.bc: main.bc rust-core/core.bc
	$(LLVM_LINK) -o - $^ | $(OPT) -mtriple=$(TARGET) $(OPTFLAGS) > $@

OUTFILES += amalgam.bc main.bc rust-core/core.bc
OUTFILES += amalgam.s main.s rust-core/core.s
OUTFILES += amalgam.o main.o rust-core/core.o

# I believe it should be possible to use llc for this step with the same result
# as clang since we've already optimized, but it seems clang has additional
# magic.
%.s: %.bc Makefile
	$(CLANG) $(CFLAGS) -S -o $@ $<
# Hack to remove 16-byte alignment for every function.
	sed -i 's/.align\s\+16/.align 1/g' $@

%.ll: %.bc
	$(LLVM_DIS) $<

rust-core/core.bc:
	cd rust-core && $(RUSTC) $(RUSTCFLAGS) --emit=bc core/lib.rs --out-dir . -Z no-landing-pads

rust-core/crate.stamp: rust-core/libcore-caef0f5f-0.0.rlib
	touch $@ --reference=$<

rust-core/libcore-caef0f5f-0.0.rlib:
	cd rust-core && $(RUSTC) $(RUSTCFLAGS) core/lib.rs --out-dir . -Z no-landing-pads
