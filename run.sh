#!/bin/sh

exec qemu-system-x86_64 -m 32M -kernel rust_kernel "$@"
