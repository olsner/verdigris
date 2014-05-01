OUT ?= out
GRUBDIR ?= $(OUT)/grub

RUSTC ?= rustc
CLANG ?= clang
CC = clang
LLVM = -3.5
LLVM_LINK = llvm-link$(LLVM)
OPT = opt$(LLVM)
LLVM_DIS = llvm-dis$(LLVM)
LLVM_AS = llvm-as$(LLVM)
AS = clang -c

TARGET = x86_64-pc-linux-elf
CFLAGS = -g -std=c99 -fomit-frame-pointer $(COPTFLAGS)
CFLAGS += --target=$(TARGET) -mcmodel=kernel -mno-red-zone -mno-sse -mno-mmx
LDFLAGS = --check-sections --gc-sections
OPT_LEVEL ?= 2
COPTFLAGS = -Oz -ffunction-sections -fdata-sections
OPTFLAGS = $(COPTFLAGS) -internalize-public-api-list=start64,syscall -internalize
RUSTCFLAGS = -g --opt-level=$(OPT_LEVEL) --dep-info $(RUSTC_DEP_OUT) --target $(TARGET)

all: $(OUT)/kernel $(OUT)/kernel.elf $(OUT)/grub.iso

clean:
	rm -fr out

KERNEL_OBJS = $(addprefix $(OUT)/, runtime.o syscall.o amalgam.o)

KERNEL_OBJS += start32.o

CORE_CRATE := $(shell $(RUSTC) $(RUSTCFLAGS) rust-core/core/lib.rs --out-dir rust-core --crate-file-name)

$(OUT)/kernel: SHELL=/bin/bash
$(OUT)/kernel: linker.ld $(KERNEL_OBJS)
	$(LD) $(LDFLAGS) -o $@ -Map $@.map -T $^
	@echo $@: `stat -c%s $@` bytes
	@echo $@: `grep fill $@.map | tr -s ' ' | cut -d' ' -f4 | while read REPLY; do echo $$[$$REPLY]; done | paste -sd+ | bc` bytes wasted on alignment
$(OUT)/kernel.elf: linker.ld $(KERNEL_OBJS)
	$(LD) $(LDFLAGS) --oformat=elf64-x86-64 -o $@ -T $^

ifdef CFG
$(OUT)/main.bc: RUSTCFLAGS += --cfg $(CFG)
endif

$(OUT)/main.bc: main.rs rust-core/$(CORE_CRATE) Makefile
	$(RUSTC) $(RUSTCFLAGS) --crate-type=lib --emit=bc -L. -Lrust-core -o $@ $<

-include $(OUT)/main.d

# Use nounwind as a dummy attribute
NO_SPLIT_STACKS = sed '/^attributes / s/ "split-stack"/ nounwind/'

$(OUT)/amalgam.bc: $(OUT)/main.bc $(OUT)/rust-core/core.bc
	$(LLVM_LINK) -o - $^ | $(OPT) -mtriple=$(TARGET) $(OPTFLAGS) | $(LLVM_DIS) | $(NO_SPLIT_STACKS) | $(LLVM_AS) > $@

# I believe it should be possible to use llc for this step with the same result
# as clang since we've already optimized, but it seems clang has additional
# magic.
$(OUT)/%.s: $(OUT)/%.bc Makefile
	$(CLANG) $(CFLAGS) -S -o $@ $<
# Hack to remove 16-byte alignment for every function.
	sed -i 's/.align\s\+16/.align 1/g' $@

$(OUT)/%.o: %.s
	@mkdir -p $(@D)
	$(AS) -o $@ $<

# Keep it around after building the .o file
.PRECIOUS: amalgam.s

%.ll: %.bc
	$(LLVM_DIS) $<

all: $(OUT)/amalgam.ll

$(OUT)/rust-core/core.bc:
	@mkdir -p $(@D)
	$(RUSTC) $(RUSTCFLAGS) --emit=bc rust-core/core/lib.rs --out-dir $(@D) -Z no-landing-pads

$(OUT)/rust-core/$(CORE_CRATE): RUSTC_DEP_OUT = $(OUT)/rust-core/crate.d
$(OUT)/rust-core/$(CORE_CRATE):
	@mkdir -p $(@D)
	$(RUSTC) $(RUSTCFLAGS) rust-core/core/lib.rs --out-dir $(@D) -Z no-landing-pads

-include $(OUT)/rust-core/core.d
-include $(OUT)/rust-core/crate.d

GRUB_MODULES = --modules="boot multiboot"

GRUB_CFG = $(GRUBDIR)/boot/grub/grub.cfg

$(GRUB_CFG): mkgrubcfg.sh
	@mkdir -p $(@D)
	bash $< > $@

$(GRUBDIR)/test.mod:
	echo -e '\x0f\x05' > $@

$(GRUBDIR)/kernel: $(OUT)/kernel
	cp -v $< $@

$(OUT)/grub.iso: $(GRUB_CFG) $(GRUBDIR)/kernel $(GRUBDIR)/test.mod
	@echo Creating grub boot image $@ from $^
	grub-mkrescue $(GRUB_MODULES) -o $@ $(GRUBDIR) >/dev/null

