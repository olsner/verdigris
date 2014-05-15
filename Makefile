SHELL = /bin/bash

.PHONY: all clean

OUT ?= out
GRUBDIR ?= $(OUT)/grub

RUSTC := $(RUST_PREFIX)/bin/rustc
CLANG ?= clang
CC = clang
LLVM = -3.5
LLVM_LINK = llvm-link$(LLVM)
OPT = opt$(LLVM)
LLVM_DIS = llvm-dis$(LLVM)
LLVM_AS = llvm-as$(LLVM)
AS = clang -c
YASM ?= yasm
ZPIPE = $(OUT)/zpipe

TARGET = x86_64-unknown-linux-gnu
CFLAGS = -g -std=c99 $(COPTFLAGS)
CFLAGS += --target=$(TARGET) -mcmodel=kernel -mno-red-zone -mno-sse -mno-mmx
LDFLAGS = --check-sections --gc-sections
OPT_LEVEL ?= 2
COPTFLAGS = -Oz -ffunction-sections -fdata-sections
PUBLIC_SYMBOLS = start64,syscall,irq_entry
OPTFLAGS = $(COPTFLAGS) -internalize-public-api-list=$(PUBLIC_SYMBOLS) -internalize
RUSTCFLAGS = -g --opt-level=$(OPT_LEVEL) --dep-info $(RUSTC_DEP_OUT) --target $(TARGET)
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

CORE_CRATE := libcore-c5ed6fb4-0.11.0-pre.rlib

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
	$(HUSH_OPT) $(LLVM_LINK) -o - $^ | $(OPT) -mtriple=$(TARGET) $(OPTFLAGS) | $(LLVM_DIS) | $(NO_SPLIT_STACKS) | $(LLVM_AS) > $@

# I believe it should be possible to use llc for this step with the same result
# as clang since we've already optimized, but it seems clang has additional
# magic.
$(OUT)/%.s: $(OUT)/%.bc Makefile
	$(HUSH_LLC) $(CLANG) $(CFLAGS) -S -o $@ $<
# Hack to remove 16-byte alignment for every function.
	@sed -i 's/.align\s\+16/.align 1/g' $@

$(OUT)/%.o: %.s
	@mkdir -p $(@D)
	$(HUSH_AS) $(AS) -o $@ $<

%.o: %.s
	$(HUSH_AS) $(AS) -o $@ $<

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
	@ar p $< $(@F).deflate | $(ZPIPE) -d > $@

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

