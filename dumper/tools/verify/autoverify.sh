#!/bin/bash -e

cleanup() {
    echo "Cleaning up..."
    rm -rf "$WORKDIR"
}
trap cleanup EXIT

echo 'Checking required programs ...'
for i in 'bash' 'rustc' 'cargo' 'python3' 'git'; do
    if ! command -v "$i"; then
        echo "ERROR: $i is not installed."
        exit 1
    fi
done

WORKDIR="$(mktemp -d)"
CURRENT_DIR="$(pwd)"
echo 'Cloning aosc-os-abbs ...'
git clone --depth=10 'https://github.com/AOSC-Dev/aosc-os-abbs.git' "$WORKDIR/aosc-os-abbs"
export SPEC_DIR="$WORKDIR/aosc-os-abbs"

echo 'Collecting reference data (sourcing in real bash) ...'
python3 "$CURRENT_DIR/dumper/tools/reference/reference_dump.py"

echo 'Building apml ...'
cargo build --release -p abbs-meta-dump
echo 'Collecting apml data ...'
# CMD_SUBST=1 executes $(...) command substitutions via `sh -c`, matching the
# reference implementation which sources the files in a real shell.
CMD_SUBST=1 SPEC_DIR="$SPEC_DIR" "$CURRENT_DIR/target/release/abbs-meta-dump" > stdout.log
echo 'Comparing implementations ...'
python3 "$CURRENT_DIR/dumper/tools/verify/verifier.py"
