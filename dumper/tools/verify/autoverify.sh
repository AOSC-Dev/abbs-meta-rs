#!/bin/bash -e

cleanup() {
    echo "Cleaning up..."
    rm -rf "$WORKDIR"
}

echo 'Checking required programs ...'
for i in 'cmake' 'gcc' 'rustc' 'cargo' 'python3' 'git' 'ninja'; do
    if ! command -v "$i"; then
        echo "ERROR: $i is not installed."
        exit 1
    fi
done

WORKDIR="$(mktemp -d)"
CURRENT_DIR="$(pwd)"
cd "$WORKDIR" || exit 2
echo 'Cloning aosc-os-abbs ...'
git clone --depth=10 'https://github.com/AOSC-Dev/aosc-os-abbs.git'
export SPEC_DIR="$WORKDIR/aosc-os-abbs"
echo 'Building bashd RPC server ...'
git clone --recursive "https://gitlab.com/liushuyu/bashd"
cd bashd && mkdir -p build && cd build
cmake .. -GNinja && ninja
cd ..
echo 'Collecting reference data ...'
python3 examples/scan-abbs.py

cd "$CURRENT_DIR"
echo 'Building apml ...'
cargo build --release
echo 'Collecting apml data ...'
./target/release/abbs-meta-dump > stdout.log
echo 'Comparing implementations ...'
python3 ./dumper/tools/verify/verifier.py

trap cleanup EXIT
