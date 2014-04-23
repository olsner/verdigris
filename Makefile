# Note: currently works with rustc 0.10 (newer rustc break rust-core)
RUSTC ?= rustc
CLANG ?= clang
CC = clang
LLVM = -3.5
LLVM_LINK = llvm-link$(LLVM)
OPT = opt$(LLVM)
LLVM_DIS = llvm-dis$(LLVM)
AS = clang -c

TARGET = x86_64-pc-linux-elf
CFLAGS = -g -std=c99 -fomit-frame-pointer $(COPTFLAGS)
CFLAGS += --target=$(TARGET) -mcmodel=kernel -mno-red-zone
LDFLAGS = --check-sections --gc-sections
OPT_LEVEL ?= 2
COPTFLAGS = -Oz -ffunction-sections -fdata-sections
OPTFLAGS = $(COPTFLAGS) -internalize-public-api-list=start64 -internalize
RUSTCFLAGS = -g --opt-level=$(OPT_LEVEL) --dep-info $(RUSTC_DEP_OUT) --target $(TARGET)

all: rust_kernel rust_kernel.elf

clean:
	rm -fr $(OUTFILES)

KERNEL_OBJS = runtime.o
KERNEL_OBJS += amalgam.o

OUTFILES :=
OUTFILES += $(KERNEL_OBJS)
OUTFILES += main.o rest-core/core.o

KERNEL_OBJS += start32.o

CORE_CRATE := $(shell $(RUSTC) $(RUSTCFLAGS) rust-core/core/lib.rs --out-dir rust-core --crate-file-name)


rust_kernel: SHELL=/bin/bash
rust_kernel: linker.ld $(KERNEL_OBJS)
	$(LD) $(LDFLAGS) -o $@ -Map $@.map -T $^
	@echo $@: `stat -c%s $@` bytes
	@echo $@: `grep fill $@.map | tr -s ' ' | cut -d' ' -f4 | while read REPLY; do echo $$[$$REPLY]; done | paste -sd+ | bc` bytes wasted on alignment
rust_kernel.elf: linker.ld $(KERNEL_OBJS)
	$(LD) $(LDFLAGS) --oformat=elf64-x86-64 -o $@ -T $^

OUTFILES += rust_kernel rust_kernel.elf rust_kernel.map

ifdef CFG
main.bc: RUSTCFLAGS += --cfg $(CFG)
endif

main.bc: main.rs rust-core/$(CORE_CRATE) Makefile
	$(RUSTC) $(RUSTCFLAGS) --crate-type=lib --emit=bc -L. -Lrust-core -o $@ $<

-include main.d

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

# Keep it around after building the .o file
.PRECIOUS: amalgam.s

%.ll: %.bc
	$(LLVM_DIS) $<

all: amalgam.ll

rust-core/core.bc:
	$(RUSTC) $(RUSTCFLAGS) --emit=bc rust-core/core/lib.rs --out-dir rust-core -Z no-landing-pads

rust-core/$(CORE_CRATE): RUSTC_DEP_OUT = rust-core/crate.d
rust-core/$(CORE_CRATE):
	$(RUSTC) $(RUSTCFLAGS) rust-core/core/lib.rs --out-dir rust-core -Z no-landing-pads

-include rust-core/core.d
-include rust-core/crate.d

