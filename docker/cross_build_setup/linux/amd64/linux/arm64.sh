#!/bin/bash -e

rust_target="aarch64-unknown-linux-gnu"

ln -s "target/$rust_target" current_target

apt-get update
apt-get -y install gcc-aarch64-linux-gnu

rustup target add "$rust_target"

mkdir -p /.cargo
cat >/.cargo/config <<EOF
[build]
target = "$rust_target"

[target.$rust_target]
linker = "aarch64-linux-gnu-gcc"
EOF