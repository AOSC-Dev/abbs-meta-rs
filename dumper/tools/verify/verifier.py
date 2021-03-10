import json
import sys


def compare_dumps(spec: bool) -> bool:
    print(f'Finding differences in `{"spec" if spec else "defines"}` ...')
    diffs = 0
    with open(f'/tmp/all_vars{"" if spec else "_def"}.json', 'rt') as f:
        reference = json.load(f)
    with open(f'/tmp/all_vars{"" if spec else "_def"}_rs.json', 'rt') as f:
        rs = json.load(f)
    for k, v in rs.items():
        ref = reference.get(k)
        if not ref:
            print(f'{k}: Missing from reference')
        if v != ref:
            print(
                f'{k}: Different from reference:\nRef: {ref}\n===\nNew: {v}\n------------------------')
            diffs += 1
    print(f'Found {diffs} differences between implementations')
    return diffs > 0


if __name__ == "__main__":
    if compare_dumps(True):
        print('Stopped. Please fix these issues first.')
        sys.exit(1)
    if compare_dumps(False):
        sys.exit(1)
