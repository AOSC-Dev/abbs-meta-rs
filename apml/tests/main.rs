use abbs_meta_apml::{lint, parse, parse_with_runner, Context, Diagnostic, DiagnosticInfo, Lint, LintFix, Value};

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
fn test_undefined_variable_warnings() {
    let has_warning = |result: &abbs_meta_apml::ParseResult| {
        result
            .warnings
            .iter()
            .any(|d| matches!(&d.info, DiagnosticInfo::Warning(m) if m.contains("never defined")))
    };

    // Referencing a never-defined variable is a warning (bash: empty), not
    // an error — like thunar's `BUILDDEP="${PKGSUG} intltool ..."`.
    let mut ctx = Context::new();
    let result = parse("BUILDDEP=\"${PKGSUG} intltool\"\n", &mut ctx);
    assert!(result.is_ok());
    assert!(has_warning(&result));
    assert_eq!(scalar(&ctx, "BUILDDEP"), " intltool");

    // Defined later in the file: no warning (forward reference is legal).
    let mut ctx = Context::new();
    let result = parse("A=$B\nB=1\n", &mut ctx);
    assert!(!has_warning(&result));

    // Defined in the initial context (e.g. from a spec): no warning.
    let mut ctx = Context::new();
    ctx.insert("__VER".into(), Value::Scalar("1.2".into()));
    let result = parse("PKGDES=\"v${__VER}\"\n", &mut ctx);
    assert!(!has_warning(&result));

    // `${name:=word}` defines the name: no warning.
    let mut ctx = Context::new();
    let result = parse("X=${Y:=fallback}\n", &mut ctx);
    assert!(!has_warning(&result));

    // Defined by `+=` before use: no warning.
    let mut ctx = Context::new();
    let result = parse("D=\"a\"\nD+=\" b\"\nE=$D\n", &mut ctx);
    assert!(!has_warning(&result));
}

#[test]
fn test_lint_undefined_variable() {
    let lints = lint("BUILDDEP=\"${PKGSUG} intltool\"\n", &Context::new());
    let rules: Vec<&str> = lints.iter().map(|l| l.rule).collect();
    assert!(rules.contains(&"undefined-variable"));
    assert!(!rules.contains(&"typo"), "PKGSUG has no close match");
}

#[test]
fn test_lint_typo_reference_direction() {
    // `BUIDDEP__NOJAVA` (reference) is a typo of the defined
    // `BUILDDEP__NOJAVA` → the reference is renamed.
    let src = "BUILDDEP__NOJAVA=\"x\"\nBUILDDEP__LOONGSON3=\"${BUIDDEP__NOJAVA}\"\n";
    let lints = lint(src, &Context::new());
    let typos: Vec<&Lint> = lints.iter().filter(|l| l.rule == "typo").collect();
    assert_eq!(typos.len(), 1, "expected exactly one typo finding");
    let fix = typos[0].fix.as_ref().expect("typo should carry a fix");
    assert_eq!(&src[fix.start..fix.end], "BUIDDEP__NOJAVA");
    assert_eq!(fix.replacement, "BUILDDEP__NOJAVA");
}

#[test]
fn test_lint_typo_definition_direction() {
    // `BUILDEP__RETRO` (definition) is a typo of the referenced
    // `BUILDDEP__RETRO` → the definition is renamed, not the references.
    let src = "BUILDEP__RETRO=\"\"\nBUILDDEP__ARMV4=\"${BUILDDEP__RETRO}\"\n";
    let lints = lint(src, &Context::new());
    let typos: Vec<&Lint> = lints.iter().filter(|l| l.rule == "typo").collect();
    assert_eq!(typos.len(), 1, "expected exactly one typo finding");
    let fix = typos[0].fix.as_ref().expect("typo should carry a fix");
    assert_eq!(&src[fix.start..fix.end], "BUILDEP__RETRO");
    assert_eq!(fix.replacement, "BUILDDEP__RETRO");
}

fn rule_findings(src: &str, rule: &str) -> Vec<Lint> {
    lint(src, &Context::new())
        .into_iter()
        .filter(|l| l.rule == rule)
        .collect()
}

#[test]
fn test_lint_pkgdes_style() {
    // Valid: uppercase start, no trailing punctuation.
    assert!(rule_findings("PKGDES=\"File manager for Xfce\"\n", "pkgdes-style").is_empty());
    // Digit start is acceptable too (e.g. product names like "3D ...").
    assert!(rule_findings("PKGDES=\"3D visualization tool for ROS 2\"\n", "pkgdes-style").is_empty());
    // Lowercase start + trailing period → two findings, each with a fix
    // (capitalize the `l`, drop the trailing `.`).
    let src = "PKGDES=\"library for rendering pdf.\"\n";
    let lints = rule_findings(src, "pkgdes-style");
    assert_eq!(lints.len(), 2, "expected start + punctuation findings: {lints:?}");
    let fixes: Vec<&LintFix> = lints.iter().filter_map(|l| l.fix.as_ref()).collect();
    assert_eq!(fixes.len(), 2, "both findings should carry fixes");
    let mut replaced: Vec<&str> = fixes.iter().map(|f| &src[f.start..f.end]).collect();
    replaced.sort_unstable();
    assert_eq!(replaced, vec![".", "l"]);
    assert!(fixes.iter().any(|f| f.replacement == "L"));
    assert!(fixes.iter().any(|f| f.replacement.is_empty()));
}

#[test]
fn test_lint_fail_arch() {
    // Valid extglob forms pass.
    assert!(rule_findings("FAIL_ARCH=\"!(mainline)\"\n", "fail-arch").is_empty());
    assert!(rule_findings("FAIL_ARCH=\"@(retro|loongson3|riscv64)\"\n", "fail-arch").is_empty());
    // Legacy plain-arch form is reported and wrapped into `@(...)`.
    let src = "FAIL_ARCH=\"loongson3\"\n";
    let lints = rule_findings(src, "fail-arch");
    assert_eq!(lints.len(), 1);
    let fix = lints[0].fix.as_ref().expect("should carry a wrap fix");
    assert_eq!(&src[fix.start..fix.end], "loongson3");
    assert_eq!(fix.replacement, "@(loongson3)");
    // Unknown architecture is reported.
    assert!(!rule_findings("FAIL_ARCH=\"!(amd64|fooarch)\"\n", "fail-arch").is_empty());
    // Dynamic (empty) values are not validated.
    assert!(rule_findings("FAIL_ARCH=\"${__CROSS}\"\n", "fail-arch").is_empty());
}

#[test]
fn test_lint_srctbl_http() {
    let src = "SRCTBL=\"http://example.com/foo.tar.xz\"\n";
    let lints = rule_findings(src, "srctbl-http");
    assert_eq!(lints.len(), 1);
    let fix = lints[0].fix.as_ref().expect("should carry an https fix");
    assert_eq!(&src[fix.start..fix.end], "http://");
    assert_eq!(fix.replacement, "https://");
    assert!(rule_findings("SRCTBL=\"https://example.com/foo.tar.xz\"\n", "srctbl-http").is_empty());
}

#[test]
fn test_lint_pkgsection() {
    // Official casing passes (autobuild4 `sets/section`, case-sensitive).
    assert!(rule_findings("PKGSEC=libs\n", "pkgsection").is_empty());
    assert!(rule_findings("PKGSEC=LXQt\n", "pkgsection").is_empty());
    assert!(rule_findings("PKGSEC=MATE\n", "pkgsection").is_empty());
    assert!(rule_findings("PKGSEC=erlang\n", "pkgsection").is_empty());
    assert!(rule_findings("PKGSEC=non-free/devel\n", "pkgsection").is_empty());
    // Non-official casing is reported and fixed to the official form.
    let src = "PKGSEC=mate\n";
    let lints = rule_findings(src, "pkgsection");
    assert_eq!(lints.len(), 1);
    let fix = lints[0].fix.as_ref().expect("should carry a fix");
    assert_eq!(&src[fix.start..fix.end], "mate");
    assert_eq!(fix.replacement, "MATE");
    assert!(!rule_findings("PKGSEC=lxqt\n", "pkgsection").is_empty());
    assert!(!rule_findings("PKGSEC=Utils\n", "pkgsection").is_empty());
    // Non-canonical with a suggestion and a fix (`util` → `utils`).
    let src = "PKGSEC=util\n";
    let lints = rule_findings(src, "pkgsection");
    assert_eq!(lints.len(), 1);
    assert!(
        lints[0].message.contains("utils"),
        "expected a `utils` suggestion, got: {}",
        lints[0].message
    );
    let fix = lints[0].fix.as_ref().expect("should carry a fix");
    assert_eq!(&src[fix.start..fix.end], "util");
    assert_eq!(fix.replacement, "utils");
    // Non-canonical without a close match.
    assert!(!rule_findings("PKGSEC=multimedia\n", "pkgsection").is_empty());
}

#[test]
fn test_lint_required_fields() {
    // A complete defines passes.
    assert!(rule_findings("PKGNAME=foo\nPKGSEC=libs\nPKGDES=\"Foo bar\"\n", "required-fields").is_empty());
    // Missing PKGDES is reported.
    let lints = rule_findings("PKGNAME=foo\nPKGSEC=libs\n", "required-fields");
    assert!(lints.iter().any(|l| l.message.contains("PKGDES")));
    // A complete spec passes.
    assert!(rule_findings("VER=1.2\nSRCS=\"tbl::https://example.com/foo-$VER.tar.xz\"\nCHKSUMS=\"sha256::abc\"\n", "required-fields").is_empty());
    // Spec missing SRCS (but has VER) is reported.
    let lints = rule_findings("VER=1.2\nREL=1\nCHKSUMS=\"sha256::abc\"\n", "required-fields");
    assert!(lints.iter().any(|l| l.message.contains("SRCS")));
    // Spec missing VER (but has SRCS) is reported.
    let lints = rule_findings("REL=1\nSRCS=\"tbl::https://example.com/foo.tar.xz\"\nCHKSUMS=\"sha256::abc\"\n", "required-fields");
    assert!(lints.iter().any(|l| l.message.contains("VER")));
    // DUMMYSRC satisfies the source requirement.
    assert!(rule_findings("VER=1.2\nDUMMYSRC=1\n", "required-fields").is_empty());
    // Arch-specific SRCS__AMD64 also satisfies the source requirement.
    assert!(rule_findings("VER=1.2\nSRCS__AMD64=\"tbl::https://example.com/foo-$VER.tar.xz\"\n", "required-fields").is_empty());
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
