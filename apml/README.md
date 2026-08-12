# abbs-meta-apml
Parser for ACBS Package Metadata Language.

See language specification in [spec.md](spec.md).

## Security

`parse()` never executes anything: command substitutions (`$( ... )`) are
parsed but expand to an empty string, so it is safe to run on untrusted
`spec` / `defines` files. The only way to obtain real command output is
[`parse_with_runner`], which executes commands found in the input — use it
only on files you trust, with a runner that sandboxes or restricts execution.
