#!/usr/bin/env python3
"""Generate ground-truth variable dumps by sourcing files in real bash.

This replaces the bashd-based reference implementation, whose `get_var` RPC
returns a raw pointer for array variables (garbage). Instead, every
`spec` / `defines` file is sourced in a clean `bash --norc` and all resulting
variables (scalars and arrays) are exported as JSON:

    scalars:  {"NAME": "value"}
    arrays:   {"NAME": ["elem0", "elem1", ...]}

Outputs:
    /tmp/all_vars.json       — spec variables
    /tmp/all_vars_def.json   — defines variables

Usage:
    SPEC_DIR=/path/to/aosc-os-abbs python3 reference_dump.py
"""

import json
import os
import subprocess
import sys

SPEC_DIR = os.environ.get("SPEC_DIR")
if not SPEC_DIR:
    print("Please set SPEC_DIR", file=sys.stderr)
    sys.exit(1)

# Variables set by bash itself (mirrors bashd's IGNORED_VARS).
# Note: match the dynamic EPOCHREALTIME/EPOCHSECONDS exactly — a bare
# `EPOCH*` glob would also swallow the packaging `EPOCH` variable.
IGNORED = (
    "BASH*|EUID|GROUPS|HOSTNAME|HOSTTYPE|IFS|LINENO|MACHTYPE|OPTERR|OPTIND|"
    "OSTYPE|PATH|PPID|PS4|PWD|RANDOM|SECONDS|SHELL|SHLVL|SRANDOM|TERM|UID|_|"
    "PIPESTATUS|BASHPID|EPOCHREALTIME|EPOCHSECONDS|HISTCMD|DIRSTACK|FUNCNAME|"
    "GLOBIGNORE|HOSTFILE|BASHOPTS|SHELLOPTS|SUPPORTED|LANG|LC_*|COMP_WORDBREAKS"
)

BASH_SCRIPT = r'''source "$1" 2>/dev/null || exit 1
for v in $(compgen -A variable); do
  case "$v" in
    {ignored}) continue;;
  esac
  decl=$(declare -p "$v" 2>/dev/null) || continue
  if [[ "$decl" == declare\ -a* ]]; then
    declare -n __ref="$v"
    printf "A\0%s\0%d\0" "$v" "${{#__ref[@]}}"
    printf '%s\0' "${{__ref[@]}}"
  elif [[ "$decl" == declare\ -A* ]]; then
    continue
  else
    printf "S\0%s\0%s\0" "$v" "${{!v}}"
  fi
done
'''.format(ignored=IGNORED)


def parse_stream(data: bytes) -> dict:
    """Parse the NUL-delimited record stream produced by BASH_SCRIPT."""
    parts = data.split(b"\0")
    out = {}
    i = 0
    n = len(parts)
    while i < n:
        if parts[i] == b"":
            # trailing artifact (or empty record — never valid at a record start)
            i += 1
            continue
        t = parts[i].decode()
        if i + 1 >= n:
            break
        name = parts[i + 1].decode()
        i += 2
        if t == "S":
            if i >= n:
                break
            out[name] = parts[i].decode()
            i += 1
        elif t == "A":
            if i >= n:
                break
            try:
                count = int(parts[i])
            except ValueError:
                break
            i += 1
            elems = []
            for _ in range(count):
                if i >= n:
                    break
                elems.append(parts[i].decode())
                i += 1
            out[name] = elems
    return out


def collect_files(spec: bool) -> list:
    """Collect `spec`/`defines` files, mirroring the Rust dumper's
    `walkdir::WalkDir::new(spec_dir).max_depth(4)` traversal."""
    target = "spec" if spec else "defines"
    files = []
    for dirpath, _dirs, names in os.walk(SPEC_DIR):
        if target not in names:
            continue
        rel = os.path.relpath(os.path.join(dirpath, target), SPEC_DIR)
        depth = len(rel.split(os.sep))
        if depth <= 4:
            files.append(os.path.join(dirpath, target))
    return sorted(files)


def has_command_substitution(content: str) -> bool:
    """True if the file contains a `$(` that a real shell would execute (and
    which the apml parser deliberately never executes).

    Mirrors dumper/tools/verify/verifier.py so both sides skip the same files:
    single quotes and comments protect a `$(`; double quotes and escapes do
    not."""
    state = "normal"  # normal | single | double | comment
    at_word_start = True
    escaped = False
    i = 0
    n = len(content)
    while i < n:
        c = content[i]
        if state == "comment":
            if c == "\n":
                state = "normal"
                at_word_start = True
            i += 1
            continue
        if state == "single":
            if c == "'":
                state = "normal"
            i += 1
            continue
        if state == "double":
            if escaped:
                escaped = False
            elif c == "\\":
                escaped = True
            elif c == '"':
                state = "normal"
            elif c == "$" and i + 1 < n and content[i + 1] == "(":
                return True
            i += 1
            continue
        # normal
        if escaped:
            escaped = False
        elif c == "\\":
            escaped = True
        elif c == "'":
            state = "single"
        elif c == '"':
            state = "double"
        elif c == "#" and at_word_start:
            state = "comment"
        elif c == "$" and i + 1 < n and content[i + 1] == "(":
            return True
        elif c in " \t\n":
            at_word_start = True
        else:
            at_word_start = False
        i += 1
    return False


def run_one(path: str, timeout: int = 60) -> subprocess.CompletedProcess:
    """Source one file in clean bash and return the completed process."""
    return subprocess.run(
        [
            "env",
            "-i",
            "PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin",
            "/bin/bash",
            "--norc",
            "-c",
            BASH_SCRIPT,
            "--",
            path,
        ],
        capture_output=True,
        timeout=timeout,
    )


def run_all(spec: bool) -> dict:
    files = collect_files(spec)
    all_vars = {}
    errors = 0
    skipped = 0
    total = len(files)
    for idx, path in enumerate(files):
        if idx % 500 == 0:
            print(f"\r[{idx}/{total}] Processing ...", end="", flush=True)
        # Files containing `$(...)` need external commands (llvm-config,
        # pkg-config, ...) that may be absent, and an array assignment whose
        # substitution fails makes `source` return non-zero when it is the
        # last statement. The verifier already skips these files, so they are
        # excluded here too.
        with open(path, "rt", errors="replace") as f:
            if has_command_substitution(f.read()):
                skipped += 1
                continue
        try:
            proc = run_one(path)
        except subprocess.TimeoutExpired:
            # A timeout is usually a transiently slow runner; retry once
            # before declaring the file unsourceable.
            try:
                proc = run_one(path)
            except subprocess.TimeoutExpired:
                print(f"\rFailure (timeout): {os.path.relpath(path, SPEC_DIR)}")
                errors += 1
                continue
        except OSError as ex:
            print(f"\rFailure: {os.path.relpath(path, SPEC_DIR)}: {ex}")
            errors += 1
            continue
        if proc.returncode != 0:
            print(f"\rFailure (bash could not source): {os.path.relpath(path, SPEC_DIR)}")
            if proc.stderr:
                print(proc.stderr.decode(errors="replace"))
            errors += 1
            continue
        key = os.path.relpath(path, SPEC_DIR)
        all_vars[key] = parse_stream(proc.stdout)
    print(f"\r[{total}/{total}] Processing ...")
    print(
        f"Total: {total}, Errors: {errors} ({errors * 100 // max(total, 1)}%), "
        f"Skipped: {skipped}"
    )
    if errors:
        # Fail the run (e.g. in CI) when bash cannot source files — this is a
        # signal that the reference data is incomplete.
        sys.exit(f"failed to source {errors} of {total} files")
    return all_vars


if __name__ == "__main__":
    print("[ spec  ] Collecting variables ...")
    with open("/tmp/all_vars.json", "wt") as f:
        json.dump(run_all(True), f)
    print("[defines] Collecting variables ...")
    with open("/tmp/all_vars_def.json", "wt") as f:
        json.dump(run_all(False), f)
