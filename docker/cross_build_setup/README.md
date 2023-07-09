# cross_build_setup

This directory contains shell scripts to facilitate Dockerfile cross compilation.

The files are organized by build & target platform, i.e. `<host os>/<host architecture>/<target os>/<target architecture>`. 

As an example, if you wanted to cross-compile from Linux AMD64 to macOS on ARM, it would run `linux/amd64/macos/arm64.sh`.

For non-cross-compilation scenarios, the files simply set up a symlink used to find the target binaries (e.g. `linux/amd64/linux/amd64.sh`). See [`native.sh`](native.sh).