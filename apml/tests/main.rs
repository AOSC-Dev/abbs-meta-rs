use abbs_meta_apml::{parse, parse_with_runner, Context, Diagnostic, DiagnosticInfo, Value};

use anyhow::{anyhow, Result};
use std::io::Read;
use std::{fs::File, path::PathBuf};

fn try_parse(content: &str) -> Result<(), Vec<Diagnostic>> {
    let mut context = Context::new();
    let result = parse(content, &mut context);
    if !result.errors.is_empty() {
        return Err(result.errors);
    }

    Ok(())
}

fn parse_into(content: &str) -> Context {
    let mut context = Context::new();
    let result = parse(content, &mut context);
    assert!(result.is_ok(), "unexpected parse errors: {result:?}");
    context
}

fn scalar<'a>(ctx: &'a Context, name: &str) -> &'a str {
    ctx.get(name).and_then(|v| v.as_scalar()).unwrap()
}

#[test]
fn parse_whole_tree() -> Result<()> {
    // Code for speed testing
    let mut defines = Vec::new();
    let mut errors = 0usize;
    let mut total = 0usize;
    let walker = walkdir::WalkDir::new(std::env::var("SPEC_DIR")?).max_depth(4);
    for entry in walker.into_iter() {
        let file = entry?;
        if file.file_name() == "spec" {
            let path = PathBuf::from(file.path());
            defines.push(path);
        }
    }

    for p in defines.into_iter() {
        let mut f = File::open(&p).unwrap();
        let mut content = String::new();
        f.read_to_string(&mut content).unwrap();
        total += 1;
        if let Err(err) = try_parse(&content) {
            errors += 1;
            println!("Got error parsing {}: {:?}", p.display(), err);
        }
    }
    println!(
        "Total: {}, Errors: {} ({}%)",
        total,
        errors,
        errors * 100 / total
    );

    Ok(())
}

#[test]
fn test_simple() -> Result<()> {
    let content = "ABC='123'\nBCD=${ABC};A__C=${BCD/3/1}\n".to_string();
    let ctx = parse_into(&content);
    assert_eq!(scalar(&ctx, "ABC"), "123");
    assert_eq!(scalar(&ctx, "BCD"), "123");
    assert_eq!(scalar(&ctx, "A__C"), "121");

    Ok(())
}

#[test]
fn test_undefined_expands_to_empty() {
    // Bash semantics: undefined variables expand to an empty string.
    let ctx = parse_into("BCD=${NO}\nPKGDEP__M68K=\"${PKGDEP__M68K}\"\n");
    assert_eq!(scalar(&ctx, "BCD"), "");
    assert_eq!(scalar(&ctx, "PKGDEP__M68K"), "");
}

#[test]
fn test_arrays() {
    let ctx = parse_into(
        "CMAKE_AFTER=(\n    '-DCMAKE_BUILD_TYPE=Release'\n    \"-DFOO=$VER\"\n    # a comment\n    '-DBAR=1'\n)\n",
    );
    match ctx.get("CMAKE_AFTER") {
        Some(Value::Array(a)) => {
            assert_eq!(a, &vec!["-DCMAKE_BUILD_TYPE=Release", "-DFOO=", "-DBAR=1"]);
        }
        other => panic!("expected array, got {other:?}"),
    }
}

#[test]
fn test_array_expansion() {
    // "..." containing only ${arr[@]} expands element-by-element.
    let ctx = parse_into("A=(x y)\nB=(\"${A[@]}\" z)\nC=\"${A[@]}\"\nD=(${A[@]})\n");
    match ctx.get("B") {
        Some(Value::Array(b)) => assert_eq!(b, &vec!["x", "y", "z"]),
        other => panic!("expected array, got {other:?}"),
    }
    assert_eq!(scalar(&ctx, "C"), "x y");
    match ctx.get("D") {
        Some(Value::Array(d)) => assert_eq!(d, &vec!["x", "y"]),
        other => panic!("expected array, got {other:?}"),
    }
}

#[test]
fn test_append() {
    let ctx = parse_into("PKGBREAK=\"a\"\nPKGBREAK+=\" b\"\nARR=(1)\nARR+=(2 3)\n");
    assert_eq!(scalar(&ctx, "PKGBREAK"), "a b");
    match ctx.get("ARR") {
        Some(Value::Array(a)) => assert_eq!(a, &vec!["1", "2", "3"]),
        other => panic!("expected array, got {other:?}"),
    }
}

#[test]
fn test_command_substitution_empty_by_default() {
    // The safe default never executes `$(...)`: it expands to an empty
    // string, like an undefined variable.
    let mut context = Context::new();
    let result = parse("A=\"$(pkg-config --libs)\"\nB=$(id -u)\n", &mut context);
    assert!(result.is_ok());
    assert_eq!(scalar(&context, "A"), "");
    assert_eq!(scalar(&context, "B"), "");
}

#[test]
fn test_empty_command_substitution_reports_warning() {
    // A skipped `$(...)` is surfaced as a warning: the value is empty not
    // because the file said so, but because the command is never executed.
    let mut ctx = Context::new();
    let result = parse("A=\"$(pkg-config --libs)\"\n", &mut ctx);
    assert!(result.is_ok());
    assert_eq!(scalar(&ctx, "A"), "");
    assert_eq!(result.warnings.len(), 1);
    let DiagnosticInfo::Warning(msg) = &result.warnings[0].info else {
        panic!("expected a warning, got {:?}", result.warnings[0].info);
    };
    assert!(msg.contains("command substitution"));
    assert_eq!(result.warnings[0].span.line, 1);
}

#[test]
fn test_runner_empty_output_is_not_a_warning() {
    // With a real runner, empty output is a legitimate command result — the
    // "(not executed)" warning only applies to the default no-exec parse.
    let mut ctx = Context::new();
    let r = parse_with_runner("A=\"$(true)\"", &mut ctx, &mut |_stages: &[Vec<
        String,
    >]| Ok(String::new()));
    assert!(r.is_ok());
    assert_eq!(scalar(&ctx, "A"), "");
    assert!(r.warnings.is_empty());
}

#[test]
fn test_command_substitution_with_runner() {
    let mut context = Context::new();
    let r = parse_with_runner(
        "A=\"$(echo hello)\"",
        &mut context,
        &mut |stages: &[Vec<String>]| {
            assert_eq!(stages, &[vec!["echo".to_string(), "hello".to_string()]]);
            Ok("hello".to_string())
        },
    );
    assert!(r.is_ok());
    assert_eq!(scalar(&context, "A"), "hello");
}

#[test]
fn test_command_substitution_pipeline() {
    let mut context = Context::new();
    let result = parse("PKGVER=1.2.3\n", &mut context);
    assert!(result.is_ok());
    let r = parse_with_runner(
        "A=\"$(echo ${PKGVER} | cut -d . -f2)\"",
        &mut context,
        &mut |stages: &[Vec<String>]| {
            assert_eq!(
                stages,
                &[
                    vec!["echo".to_string(), "1.2.3".to_string()],
                    vec![
                        "cut".to_string(),
                        "-d".to_string(),
                        ".".to_string(),
                        "-f2".to_string()
                    ],
                ]
            );
            Ok("2".to_string())
        },
    );
    assert!(r.is_ok());
    assert_eq!(scalar(&context, "A"), "2");
}

#[test]
fn test_multiline_continuation() {
    let ctx = parse_into(
        "PKGDEP=\"llvm elfutils libedit ethtool luajit python-3 netaddr libbpf arping \\\n        iperf3 netperf\"\n",
    );
    assert_eq!(
        scalar(&ctx, "PKGDEP"),
        "llvm elfutils libedit ethtool luajit python-3 netaddr libbpf arping         iperf3 netperf"
    );
}

#[test]
fn test_substitutions() {
    let ctx = parse_into(
        "VER=1.2.3\nA=${VER//./_}\nB=${VER%.*}\nC=${VER#1.}\nD=${VER:-4.0}\nE=${VER:0:1}\nF=${VER^^}\nG=${#VER}\n",
    );
    assert_eq!(scalar(&ctx, "A"), "1_2_3");
    assert_eq!(scalar(&ctx, "B"), "1.2");
    assert_eq!(scalar(&ctx, "C"), "2.3");
    assert_eq!(scalar(&ctx, "D"), "1.2.3");
    assert_eq!(scalar(&ctx, "E"), "1");
    assert_eq!(scalar(&ctx, "F"), "1.2.3");
    assert_eq!(scalar(&ctx, "G"), "5");
}

#[test]
fn test_arithmetic() {
    let ctx = parse_into("A=$((1+2*3))\nB=$(( (A) * 2 ))\n");
    assert_eq!(scalar(&ctx, "A"), "7");
    assert_eq!(scalar(&ctx, "B"), "14");
}

#[test]
fn test_comments_and_blank_lines() {
    let ctx = parse_into("# leading comment\n\nA=1 # trailing\nB=2\n");
    assert_eq!(scalar(&ctx, "A"), "1");
    assert_eq!(scalar(&ctx, "B"), "2");
}

#[test]
fn test_quoted_parens_are_literal() {
    let ctx = parse_into("FAIL_ARCH=\"!(amd64|arm64|loongarch64)\"\n");
    assert_eq!(scalar(&ctx, "FAIL_ARCH"), "!(amd64|arm64|loongarch64)");
}

#[test]
fn test_pkgsrc_style_spec() {
    let ctx = parse_into(
        "VER=16.1.0\nSRCS=\"https://sourceware.org/pub/gcc/releases/gcc-${VER}/gcc-${VER}.tar.xz\"\n",
    );
    assert_eq!(
        scalar(&ctx, "SRCS"),
        "https://sourceware.org/pub/gcc/releases/gcc-16.1.0/gcc-16.1.0.tar.xz"
    );
}

#[test]
fn test_commands_not_allowed() {
    let mut context = Context::new();
    assert!(parse("echo hello\n", &mut context).is_err());
    assert!(parse("a=b && c=d\n", &mut context).is_err());
}

#[test]
fn test_self_reference_and_forward_reference() {
    // man-db style self reference; forward reference to a later variable.
    let ctx = parse_into("PKGDEP__M68K=\"${PKGDEP__M68K}\"\nBUILDDEP=\"x\"\nA=\"$BUILDDEP\"\n");
    assert_eq!(scalar(&ctx, "PKGDEP__M68K"), "");
    assert_eq!(scalar(&ctx, "A"), "x");
}

#[test]
fn test_unsupported_syntax_errors() {
    let cases = [
        "'unterminated",    // unterminated single quote
        "A=\"unterminated", // unterminated double quote
        "A=(",              // unterminated array
        "A=${",             // unterminated braced expansion
    ];
    for c in cases {
        let mut context = Context::new();
        assert!(parse(c, &mut context).is_err(), "expected error for {c:?}");
    }
}

#[test]
fn test_pretty_print_does_not_panic() {
    // Long files where the error marker might exceed the source length used
    // to panic in the snippet renderer.
    let content = "A=1\nB=2\nC=3\nD=4\nE=5\nF=\"$(unterminated\n";
    let mut context = Context::new();
    let result = parse(content, &mut context);
    for e in result.errors {
        // Should not panic even when positions are off.
        let _ = e.pretty_print(content, "test");
    }
    let _ = anyhow!("ok");
}

#[test]
fn test_array_element_with_quoted_spaces_not_split() {
    // `--with-lisp='sbcl --dynamic-space-size 4096'` and
    // `--with-blas-libs="-lcblas -llapack -lgomp"` keep their spaces: word
    // splitting only happens at unquoted-expansion whitespace.
    let ctx = parse_into(
        "A=(--with-lisp='sbcl --dynamic-space-size 4096' --enable-gmp)\n\
         B=(--with-blas-libs=\"-lcblas -llapack -lgomp\")\n",
    );
    match ctx.get("A") {
        Some(Value::Array(a)) => {
            assert_eq!(
                a,
                &vec!["--with-lisp=sbcl --dynamic-space-size 4096", "--enable-gmp"]
            );
        }
        other => panic!("expected array, got {other:?}"),
    }
    match ctx.get("B") {
        Some(Value::Array(b)) => assert_eq!(b, &vec!["--with-blas-libs=-lcblas -llapack -lgomp"]),
        other => panic!("expected array, got {other:?}"),
    }
}

#[test]
fn test_array_element_split_at_unquoted_expansion() {
    // Unquoted expansions do split: `pre${X}post` with X="a b" → ["prea", "bpost"].
    let ctx = parse_into("X=\"a b\"\nA=(pre${X}post)\n");
    match ctx.get("A") {
        Some(Value::Array(a)) => assert_eq!(a, &vec!["prea", "bpost"]),
        other => panic!("expected array, got {other:?}"),
    }
}

#[test]
fn test_array_element_substitution() {
    // `${arr[@]/pat/rep}` applies the substitution to every element.
    let ctx = parse_into("A=(x-1 y-2)\nB=(\"${A[@]/-/\\/}\")\nC=(\"${A[@]^^}\")\n");
    match ctx.get("B") {
        Some(Value::Array(b)) => assert_eq!(b, &vec!["x/1", "y/2"]),
        other => panic!("expected array, got {other:?}"),
    }
    match ctx.get("C") {
        Some(Value::Array(c)) => assert_eq!(c, &vec!["X-1", "Y-2"]),
        other => panic!("expected array, got {other:?}"),
    }
}

#[test]
fn test_tilde_expansion_in_replacement() {
    // Bash tilde-expands an unquoted `~` replacement.
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".to_string());
    let ctx = parse_into("V=abc-def\nA=${V/-/~}\n");
    assert_eq!(scalar(&ctx, "A"), format!("abc{home}def"));
}

#[test]
fn test_escaped_tilde_not_expanded() {
    // A `\~` (escaped) replacement is not tilde-expanded.
    let ctx = parse_into("V=abc-def\nA=${V/\\-/\\~}\n");
    assert_eq!(scalar(&ctx, "A"), "abc~def");
}

#[test]
fn test_assignment_requires_no_whitespace() {
    // In bash `a=b` is an assignment, while `a = b` is a command named `a`.
    // Since commands are forbidden in apml, only the no-whitespace form is
    // an assignment; the spaced forms must be rejected.
    let mut context = Context::new();
    let result = parse("a=b\n", &mut context);
    assert!(result.is_ok());
    assert_eq!(scalar(&context, "a"), "b");

    // `a = b`, `a =b`, `a =b=c` are all commands → the whole file is rejected.
    for bad in ["a = b\n", "a =b\n", "a =b=c\n", "a= b\n"] {
        let mut ctx = Context::new();
        assert!(parse(bad, &mut ctx).is_err(), "expected error for {bad:?}");
        // Nothing is applied when the file contains a syntax error.
        assert!(!ctx.contains_key("a"), "expected no assignment for {bad:?}");
    }
}
