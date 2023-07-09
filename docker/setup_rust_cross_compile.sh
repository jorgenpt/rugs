#!/bin/bash -e

host_os="$1"
host_arch="$2"
target_os="$3"
target_arch="$4"

rust_cross_linker=""
case "$target_os" in
    linux)
        case "$target_arch" in
            amd64)
                rust_target="x86_64-unknown-linux-gnu"
                ;;
            arm64)
                rust_target="aarch64-unknown-linux-gnu"
                rust_cross_linker="aarch64-linux-gnu-gcc"
                ;;
            *)
                echo "Unsupported architecture $target_arch for $target_os" >&2
                exit 1
                ;;
        esac
        ;;
    darwin)
        case "$target_arch" in
            amd64)
                rust_target="x86_64-apple-darwin"
                ;;
            arm64)
                rust_target="aarch64-apple-darwin"
                ;;
            *)
                echo "Unsupported architecture $target_arch for $target_os" >&2
                exit 1
                ;;
        esac
        ;;
esac

mkdir -p target/$rust_target
ln -s $rust_target target/current_target

mkdir -p /.cargo
cat >/.cargo/config <<EOF
[build]
target = "$rust_target"

EOF

if [[ "$host_os" == "$target_os" && "$host_arch" == "$target_arch" ]]; then
    echo "Native build, doing nothing"
    exit 0
fi

if [[ "$host_os" != "linux" ]]; then
    echo "No cross-compile set up for host OS $host_os" >&2
    exit 1
fi

if [[ "$rust_cross_linker" != "" ]]; then

    cat >>/.cargo/config <<EOF
[target.$rust_target]
linker = "$rust_cross_linker"

EOF
fi

rustup target add "$rust_target"

if [[ "$target_arch" == "arm64" ]]; then
    apt-get update
    apt-get -y install gcc-aarch64-linux-gnu
else
    echo "Unsupported architecture $target_arch" >&2
    exit 1
fi