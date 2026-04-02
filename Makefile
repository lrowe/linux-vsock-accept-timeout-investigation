.PHONY: all clean run

ARCH := $(shell uname -m)
FIRECRACKER_CI = https://s3.amazonaws.com/spec.ccfc.min/firecracker-ci/20260318-4392a8d19ab0-0/x86_64
FIRECRACKER_VERSION = v1.15.0
FIRECRACKER_SUFFIX = -$(FIRECRACKER_VERSION)-$(ARCH)

all: target/firecracker target/initrd.cpio

run: target/firecracker target/vmlinux-6.1.164 target/initrd.cpio
	trap "rm -f target/v.sock" EXIT; target/firecracker --no-api --config-file vmconfig.json

clean:
	rm -rf target

target/release/minimal target/release/httpserver target/release/httpserversync &: Cargo.toml Cargo.lock src/bin/*.rs
	cargo build --release

target/vmlinux-6.1.164:
	mkdir -p target
	curl -L -o $@ $(FIRECRACKER_CI)/vmlinux-6.1.164

target/firecracker:
	rm -rf target/release$(FIRECRACKER_SUFFIX) target/firecracker
	mkdir -p target
	curl -L https://github.com/firecracker-microvm/firecracker/releases/download/$(FIRECRACKER_VERSION)/firecracker$(FIRECRACKER_SUFFIX).tgz | tar -xz -C target
	ln -s release$(FIRECRACKER_SUFFIX)/firecracker$(FIRECRACKER_SUFFIX) target/firecracker

target/initrd.cpio: Dockerfile init.sh target/release/minimal
	rm -f $@
	docker buildx build --output type=tar,dest=- . | bsdtar -cf - --format=newc @- > $@
