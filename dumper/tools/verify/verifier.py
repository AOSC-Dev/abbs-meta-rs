import json
import sys


def compare_dumps(spec: bool) -> bool:
    print(f'Finding differences in `{"spec" if spec else "defines"}` ...')
    diffs = 0
    ref_path = '/tmp/all_vars.json' if spec else '/tmp/all_vars_def.json'
    rs_path = '/tmp/all_vars_rs.json' if spec else '/tmp/all_vars_def_rs.json'
    with open(ref_path, 'rt') as f:
        reference = json.load(f)
    with open(rs_path, 'rt') as f:
        rs = json.load(f)

    for k, v in rs.items():
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
        if k not in rs:
            print(f'{k}: Present in reference but missing from Rust dump')
            diffs += 1

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
