#!/bin/sh

exec qemu-system-x86_64 -cpu SandyBridge -m 32M -cdrom out/grub.iso "$@"
