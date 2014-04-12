RUSTC ?= rustc
CLANG ?= clang
CC = clang

TARGET = x86_64-pc-linux-elf
CFLAGS = -std=c99 -fomit-frame-pointer -Oz -ffunction-sections -fdata-sections
CFLAGS += --target=$(TARGET)
#CFLAGS += -Wa,--no-target-align
LDFLAGS = --check-sections --gc-sections
OPT_LEVEL ?= 2

all: rust_kernel rust_kernel.elf

rust_kernel: SHELL=/bin/bash
rust_kernel: linker.ld start32.o main.o rust-core/core.o
	$(LD) $(LDFLAGS) -o $@ -Map $@.map -T $^
	@echo $@: `stat -c%s $@` bytes
	@echo $@: `grep fill $@.map | tr -s ' ' | cut -d' ' -f4 | while read REPLY; do echo $$[$$REPLY]; done | paste -sd+ | bc` bytes wasted on alignment
rust_kernel.elf: linker.ld start32.o main.o rust-core/core.o
	$(LD) $(LDFLAGS) --oformat=elf64-x86-64 -o $@ -T $^
clean::; rm -f rust_kernel main.o

main.bc: main.rs rust-core/crate.stamp Makefile
	$(RUSTC) --opt-level=$(OPT_LEVEL) --target $(TARGET) --crate-type=lib --emit=bc -L. -Lrust-core -o $@ $<
clean::; rm -f main.bc

main.bc: mboot.rs start32.rs

%.s: %.bc Makefile
	$(CLANG) $(CFLAGS) -S -o $@ $<
# Hack to remove 16-byte alignment for every function.
	sed -i 's/.align\s\+16/.align 1/g' $@

rust-core/core.bc:
	cd rust-core && rustc --emit=bc core/lib.rs --out-dir . -O -Z no-landing-pads

rust-core/crate.stamp: rust-core/libcore-caef0f5f-0.0.rlib
	touch $@ --reference=$<

rust-core/libcore-caef0f5f-0.0.rlib:
	cd rust-core && rustc core/lib.rs --out-dir . -O -Z no-landing-pads
