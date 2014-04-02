CFLAGS = -std=c99 -fomit-frame-pointer -Os
LDFLAGS = --check-sections --gc-sections

all: test.o test_kernel

test_kernel: linker.ld start32.o test.o
	$(LD) $(LDFLAGS) -o $@ -Map $@.map -T $^
