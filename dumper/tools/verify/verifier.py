import json
import os
import sys

SPEC_DIR = os.environ.get("SPEC_DIR")


def has_command_substitution(content: str) -> bool:
    """True if the file contains a `$(` that a real shell would execute (and
    which the apml parser deliberately never executes).

    Tracks single quotes, double quotes, comments and escapes: a `$(` is
    executable when unquoted or inside double quotes (single quotes and
    comments protect it; an escaped `\\$(` is literal)."""
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


def compare_dumps(spec: bool) -> bool:
    print(f'Finding differences in `{"spec" if spec else "defines"}` ...')
    diffs = 0
    skipped = []
    ref_path = '/tmp/all_vars.json' if spec else '/tmp/all_vars_def.json'
    rs_path = '/tmp/all_vars_rs.json' if spec else '/tmp/all_vars_def_rs.json'
    with open(ref_path, 'rt') as f:
        reference = json.load(f)
    with open(rs_path, 'rt') as f:
        rs = json.load(f)

    def is_skipped(key: str) -> bool:
        # Files containing command substitution are excluded: the Rust parser
        # refuses to execute `$(...)` (they expand to empty), while real bash
        # executes them, so their values are expected to differ.
        if not SPEC_DIR:
            return False
        path = os.path.join(SPEC_DIR, key)
        try:
            with open(path, 'rt', errors='replace') as f:
                return has_command_substitution(f.read())
        except OSError:
            return False

    for k, v in rs.items():
        if is_skipped(k):
            skipped.append(k)
            continue
        if k not in reference:
            print(f'{k}: Present in Rust dump but missing from reference')
            diffs += 1
        elif reference[k] != v:
            print(
                f'{k}: Different from reference:\n'
                f'------------------------\n'
                f'Ref: {json.dumps(reference[k], ensure_ascii=False)}\n'
                f'===\n'
                f'New: {json.dumps(v, ensure_ascii=False)}\n'
                f'------------------------')
            diffs += 1
    for k in reference:
        if is_skipped(k):
            continue
        if k not in rs:
            print(f'{k}: Present in reference but missing from Rust dump')
            diffs += 1

    skipped = sorted(set(skipped))
    # Confirm both sides skip the same files: reference_dump.py persists its
    # skip list, so a divergence between the two has_command_substitution
    # copies is caught here instead of silently corrupting the comparison.
    suffix = '' if spec else '_def'
    try:
        with open(f'/tmp/all_vars{suffix}_skipped.json', 'rt') as f:
            ref_skipped = sorted(json.load(f))
    except OSError:
        ref_skipped = None
    if ref_skipped is not None and ref_skipped != skipped:
        print('Skip list mismatch between reference and verifier:')
        only_ref = sorted(set(ref_skipped) - set(skipped))
        only_ver = sorted(set(skipped) - set(ref_skipped))
        if only_ref:
            print(f'  skipped by reference only: {only_ref}')
        if only_ver:
            print(f'  skipped by verifier only:  {only_ver}')
        diffs += 1

    print(
        f'Skipped {len(skipped)} files containing command substitution (`$(...)`):')
    for k in skipped:
        print(f'  {k}')
    print(f'Found {diffs} differences between implementations')
    return diffs > 0


if __name__ == "__main__":
    fail = False
    if compare_dumps(True):
        print('Stopped. Please fix these issues first.')
        fail = True
    if compare_dumps(False):
        fail = True
    sys.exit(1 if fail else 0)
