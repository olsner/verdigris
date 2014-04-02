CFLAGS = -std=c99 -fomit-frame-pointer -Os
LDFLAGS = --check-sections --gc-sections
RUSTC ?= rustc
OPT_LEVEL ?= 2
TARGET = x86_64-intel-linux

all: rust_kernel

rust_kernel: linker.ld start32.o main.o
	$(LD) $(LDFLAGS) -o $@ -Map $@.map -T $^

main.o: main.rs Makefile
	$(RUSTC) --opt-level=$(OPT_LEVEL) --target $(TARGET) --crate-type=lib --emit=obj -L. -o $@ $<

main.o: mboot.rs start32.rs
