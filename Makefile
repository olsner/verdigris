SHELL = /bin/bash

.PHONY: all clean

OUT ?= out
GRUBDIR ?= $(OUT)/grub

# RUST_PREFIX must be set pointing to the prefix where a nice nightly is
# installed. This was tested with the 2014-08-28 build.
# Default: make a symlink rust-nightly in the project root, pointing to Rust.
RUST_PREFIX ?= rust-nightly
RUSTC := $(RUST_PREFIX)/bin/rustc
export LD_LIBRARY_PATH=$(RUST_PREFIX)/lib
CLANG ?= clang-3.5
CC = $(CLANG)
LLVM = -3.5
LLVM_LINK = llvm-link$(LLVM)
OPT = opt$(LLVM)
LLVM_DIS = llvm-dis$(LLVM)
LLVM_AS = llvm-as$(LLVM)
AS = $(CLANG) -c
YASM ?= yasm
ZPIPE = $(OUT)/zpipe

TARGET = x86_64-unknown-linux-gnu
CFLAGS = -g -std=c99 -Oz -ffunction-sections -fdata-sections
CFLAGS += --target=$(TARGET) -mcmodel=kernel -mno-red-zone -mno-sse -mno-mmx
CFLAGS += -ffreestanding $(COPTFLAGS)
COPTFLAGS = -fno-unroll-loops -freroll-loops -funit-at-a-time
COPTFLAGS += -mllvm -exhaustive-register-search
LDFLAGS = --check-sections --gc-sections
PUBLIC_SYMBOLS = start64,syscall,irq_entry
OPTFLAGS = -Oz -function-sections -data-sections
OPTFLAGS += -disable-internalize -std-link-opts
OPTFLAGS += -internalize-public-api-list=$(PUBLIC_SYMBOLS) -internalize
OPTFLAGS += -argpromotion -mergefunc -deadargelim
RUSTCFLAGS = -g -O --dep-info $(RUSTC_DEP_OUT) --target $(TARGET)
RUSTLIBS = -L.

CP = @cp
ifeq ($(VERBOSE),YES)
CP = @cp -v
else
# Why doesn't asmos/Makefile need any -e flags?
HUSH_AS = @echo -e      ' [AS]\t'$@;
HUSH_ASM = @echo -e     ' [ASM]\t'$@;
HUSH_ASM_DEP = @echo    ' [DEP]\t'$@;
HUSH_ASM_DEP = @
HUSH_CC = @echo -e     ' [CC]\t'$@;
HUSH_CXX = @echo -e     ' [CXX]\t'$@;
HUSH_LD  = @echo -e     ' [LD]\t'$@;
HUSH_RUST = @echo -e    ' [RUST]\t'$@;
HUSH_OPT = @echo -e     ' [OPT]\t'$@;
HUSH_LLC = @echo -e    ' [LLC]\t'$@;
hush = @echo -e       ' [$1]\t'$@;
HUSH_DIS=@echo -e     ' [DIS]\t'$@;
endif

all: $(OUT)/kernel $(OUT)/kernel.elf $(OUT)/grub.iso

clean:
	rm -fr out

KERNEL_OBJS = $(addprefix $(OUT)/, runtime.o syscall.o amalgam.o)

KERNEL_OBJS += start32.o

CORE_CRATE := libcore-4e7c5e5c.rlib

$(OUT)/kernel.elf: linker.ld $(KERNEL_OBJS)
	$(HUSH_LD) $(LD) $(LDFLAGS) --oformat=elf64-x86-64 -o $@ -T $^ -Map $(@:.elf=.map)
	@echo $@: `grep fill $(@:.elf=.map) | tr -s ' ' | cut -d' ' -f4 | while read REPLY; do echo $$[$$REPLY]; done | paste -sd+ | bc` bytes wasted on alignment
$(OUT)/kernel: $(OUT)/kernel.elf
	$(call hush,OBJCOPY) objcopy -O binary $< $@
	@echo $@: `stat -c%s $@` bytes

-include $(OUT)/syscall.d

$(OUT)/main.bc: main.rs $(OUT)/rust-core/$(CORE_CRATE) Makefile
	$(HUSH_RUST) $(RUSTC) $(RUSTCFLAGS) $(if $(CFG),--cfg $(CFG)) --crate-type=lib --emit=bc $(RUSTLIBS) -o $@ $<

-include $(OUT)/main.d

# Use nounwind as a dummy attribute
NO_SPLIT_STACKS = sed '/^attributes / s/ "split-stack"/ nounwind/'

$(OUT)/amalgam.bc: $(OUT)/main.bc $(OUT)/rust-core/core.bc
	$(HUSH_OPT) $(LLVM_LINK) -o - $^ | $(LLVM_DIS) | $(NO_SPLIT_STACKS) | $(LLVM_AS) | $(OPT) -mtriple=$(TARGET) $(OPTFLAGS) > $@

# I believe it should be possible to use llc for this step with the same result
# as clang since we've already optimized, but it seems clang has additional
# magic.
$(OUT)/%.s: $(OUT)/%.bc Makefile
	$(HUSH_LLC) $(CLANG) $(CFLAGS) -S -o $@ $<
# Hack to remove 16-byte alignment for every function.
	@sed -i 's/.align\s\+16/.align 1/g' $@

$(OUT)/%.o: %.s
	@mkdir -p $(@D)
	$(HUSH_AS) as -g -o $@ $<

%.o: %.s
	$(HUSH_AS) $(AS) $(ASFLAGS) -o $@ $<

$(OUT)/%.o: %.asm
	@mkdir -p $(@D)
	$(HUSH_ASM_DEP) $(YASM) -i . -e -M $< -o $@ > $(@:.o=.d)
	$(HUSH_ASM) $(YASM) -i . -f elf64 -g dwarf2 $< -o $@ -L nasm -l $(OUT)/$*.lst

# Keep it around after building the .o file
.PRECIOUS: $(OUT)/amalgam.s

%.ll: %.bc
	$(HUSH_DIS) $(LLVM_DIS) $<

all: $(OUT)/amalgam.ll

$(ZPIPE): zpipe.c
	$(HUSH_CC) $(CC) -lz -o $@ $<

RUST_LIBDIR = $(RUST_PREFIX)/lib/rustlib/x86_64-unknown-linux-gnu/lib
$(OUT)/rust-core/core.bc: $(RUST_LIBDIR)/$(CORE_CRATE) $(ZPIPE)
	@mkdir -p $(@D)
	ar p $< $(CORE_CRATE:lib%.rlib=%).bytecode.deflate | tail -c +24 | $(ZPIPE) > $@.tmp
	mv $@.tmp $@

$(OUT)/rust-core/$(CORE_CRATE): $(RUST_LIBDIR)/$(CORE_CRATE)
	@mkdir -p $(@D)
	@$(CP) $< $@

GRUB_MODULES = --modules="boot multiboot"

GRUB_CFG = $(GRUBDIR)/boot/grub/grub.cfg

$(GRUB_CFG): mkgrubcfg.sh
	@mkdir -p $(@D)
	bash $< > $@

$(GRUBDIR)/test.mod: test.asm
	$(HUSH_ASM) $(YASM) -f bin -L nasm -o $@ $<

$(GRUBDIR)/kernel: $(OUT)/kernel
	@$(CP) $< $@

$(OUT)/grub.iso: $(GRUB_CFG) $(GRUBDIR)/kernel $(GRUBDIR)/test.mod
	@echo Creating grub boot image $@ from $^
	grub-mkrescue $(GRUB_MODULES) -o $@ $(GRUBDIR) >/dev/null

