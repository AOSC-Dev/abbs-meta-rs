//! A lint framework for `spec` / `defines` files.
//!
//! Rules run over the parsed AST and report [`Lint`] findings, each with a
//! source span and — where the fix is unambiguous — a [`LintFix`] that a
//! caller (e.g. the `abbs-lint` CLI) can apply with `--fix`.
//!
//! The current rule set:
//!
//! * `syntax` — parse errors (severity: error, no fix).
//! * `undefined-variable` — a variable is referenced but never defined
//!   (severity: warning, no fix; Bash would expand it to the empty string).
//! * `typo` — an undefined variable is within one edit of a defined or
//!   framework variable name, so it is almost certainly a typo (severity:
//!   warning, fix: rename the reference or the definition).
//! * `pkgdes-style` — PKGDES should start with an uppercase letter or a
//!   digit (e.g. `3D ...`) and not end with punctuation
//!   (package-styling-manual §2.3).
//! * `fail-arch` — FAIL_ARCH must be an extglob expression
//!   `@(arch|...)` / `!(arch|...)` over known architectures and groups.
//! * `srctbl-http` — SRCTBL uses insecure `http://` / `ftp://` (QA W112).
//! * `pkgsection` — PKGSEC is not a canonical section (autobuild
//!   `sets/section`), with a close-match suggestion.
//!
//! Rules are pure static analysis: `lint` does not evaluate the file, so it
//! does not mutate `context`. Evaluation (and the `$( ... )` warning) is
//! the job of [`super::parse`]. Rules that need a variable's final value
//! evaluate just that variable with [`eval_var`].

use super::ast::*;
use super::error::{DiagnosticInfo, DiagnosticSpan};
use super::eval::{self, Context};
use super::parser;
use super::undefined::{self, VarRef, FRAMEWORK_VARS};
use std::collections::{HashMap, HashSet};

/// How serious a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Error,
    Warning,
}

/// A source edit that fixes a [`Lint`] finding.
///
/// `start..end` is a byte range into the original source; `replacement`
/// replaces that range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintFix {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// A single lint finding.
#[derive(Debug, Clone)]
pub struct Lint {
    /// The rule that produced this finding, e.g. `"undefined-variable"`.
    pub rule: &'static str,
    pub severity: LintSeverity,
    pub span: DiagnosticSpan,
    pub message: String,
    /// The suggested fix, when the rule can produce one.
    pub fix: Option<LintFix>,
}

/// Run every lint rule over `c`, using `context` as the set of variables
/// defined *outside* this file (e.g. the variables collected from a `spec`
/// when linting a `defines` file).
pub fn lint(c: &str, context: &Context) -> Vec<Lint> {
    let stmts = match parser::parse_program(c) {
        Ok(stmts) => stmts,
        Err(errors) => {
            return errors
                .into_iter()
                .map(|d| Lint {
                    rule: "syntax",
                    severity: LintSeverity::Error,
                    span: d.span,
                    message: match &d.info {
                        DiagnosticInfo::Error(e) => e.to_string(),
                        DiagnosticInfo::Warning(_) => String::new(),
                    },
                    fix: None,
                })
                .collect();
        }
    };

    let mut out = Vec::new();
    out.extend(rule_undefined_variable(&stmts, context));
    out.extend(rule_typo(c, &stmts, context));
    out.extend(rule_pkgdes_style(&stmts, context));
    out.extend(rule_fail_arch(&stmts, context));
    out.extend(rule_srctbl_http(&stmts, context));
    out.extend(rule_pkgsection(&stmts, context));
    out.extend(rule_required_fields(c, &stmts));
    out
}

/// `undefined-variable`: referenced but never defined — neither in this file
/// nor in the initial `context`, and not a framework/build-environment
/// variable. Reported once per name, at the first reference.
fn rule_undefined_variable(stmts: &[Stmt], context: &Context) -> Vec<Lint> {
    let defined = undefined::collect_defined(stmts, context);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for r in undefined::collect_refs(stmts) {
        if !defined.contains(&r.name)
            && !FRAMEWORK_VARS.contains(&r.name.as_str())
            && seen.insert(r.name.clone())
        {
            out.push(Lint {
                rule: "undefined-variable",
                severity: LintSeverity::Warning,
                span: r.span.into(),
                message: format!("variable `{}` referenced but never defined", r.name),
                fix: None,
            });
        }
    }
    out
}

/// Well-known ABBS variable names (base names), from the Autobuild3 manual
/// and the package styling manual. `base__ARCH` variants of these are also
/// recognised. Used to decide the *direction* of a typo fix: when an
/// undefined reference and a defined name are one edit apart, the side that
/// is not a known ABBS name is the typo.
const KNOWN_BASES: &[&str] = &[
    // Package metadata (defines).
    "PKGNAME", "PKGVER", "PKGREL", "PKGDES", "PKGSEC", "PKGCAT", "PKGDEP", "BUILDDEP",
    "PKGSUG", "PKGPROV", "PKGRECOM", "PKGREP", "PKGBREAK", "PKGCONFL", "PKGCONFIG",
    // Spec file.
    "VER", "REL", "EPOCH", "SRCS", "SRCTBL", "CHKSUMS", "CHKUPDATE", "DUMMYSRC",
    "UPSTREAM_VER", "SUBDIR", "SUB",
    // Autobuild-provided paths and metadata.
    "SRCDIR", "BLDDIR", "PKGDIR", "SYMDIR", "ABHOST", "ABTYPE", "ABPY3VER",
    // Build environment switches (Autobuild3 manual §2.1).
    "NOLTO", "NOTEST", "ABTEST_AUTO_DETECT", "USECLANG", "ABSHADOW", "ABCONFIGHACK",
    "ABCLEAN", "ABTHREADS", "NOPARALLEL", "ABSTRIP", "ABSPLITDBG", "ABELFDEP",
    "ABIFLAGS", "ABOPTS", "RECONF", "FAIL_ARCH", "VER_NONE", "MAKE_AFTER",
    "AB_FLAGS_O3", "AB_FLAGS_SPECS", "AB_FLAGS_SSP", "AB_FLAGS_FTF", "AB_FLAGS_RRO",
    "AB_FLAGS_PIE", "AB_FLAGS_PIC", "NOPYTHON2", "NOPYTHON3", "QT_SELECT",
    // Build-type specific arguments (*_DEF / *_AFTER).
    "AUTOTOOLS_DEF", "AUTOTOOLS_AFTER", "CMAKE_DEF", "CMAKE_AFTER", "MESON_DEF",
    "MESON_AFTER", "WAF_DEF", "WAF_AFTER", "QTPROJ_DEF", "QTPROJ_AFTER",
    "CARGO_AFTER", "GO_BUILD_AFTER", "GNUMAKE_AFTER", "PERL_AFTER", "PYTHON_AFTER",
    "GO_AFTER", "NODEJS_AFTER",
    // Autobuild test framework (ABTESTS plus per-test metadata).
    "ABTESTS", "ABTEST_AUTO_DETECT_ANCHOR", "TESTDES", "TESTDEP", "TESTEXEC",
    "TESTTYPE",
    // Python version helpers (AB2/3VER, AB2/3SHORTVER).
    "AB2VER", "AB3VER", "AB2SHORTVER", "AB3SHORTVER",
];

/// Architectures recognised in `FAIL_ARCH` expressions.
const KNOWN_ARCHES: &[&str] = &[
    "amd64", "arm64", "armv4", "armv6hf", "armv7hf", "i486", "loongarch64",
    "loongarch64_nosimd", "loongson2f", "loongson3", "m68k", "powerpc", "ppc64",
    "ppc64el", "riscv64", "mips64r6el", "optenv32", "alpha", "ia64", "sparc64",
    "hppa", "mips", "mips32", "mips64el", "sh4", "s390x", "x86", "i386",
];

/// Architecture groups allowed in `FAIL_ARCH` expressions (from
/// `autobuild4/sets/arch_groups.json`).
const ARCH_GROUPS: &[&str] = &[
    "mainline", "mainline_tier1", "mainline_tier2", "arm", "ocaml_native", "retro",
    "optenv", "64bit", "32bit",
];

/// Canonical `PKGSEC` sections (autobuild's `sets/section`, plus modern
/// additions used across the tree). `non-free/<s>` and `contrib/<s>` forms
/// of these are accepted too.
const KNOWN_SECTIONS: &[&str] = &[
    "admin", "Bases", "Cinnamon", "cli-mono", "comm", "cryptocurrency", "Cutefish",
    "database", "debian-installer", "debug", "devel", "doc", "editors", "electronics",
    "embedded", "fonts", "games", "gnome", "gnu-r", "gnustep", "graphics", "hamradio",
    "haskell", "httpd", "interpreters", "java", "kde", "kernel", "libdevel", "libs",
    "lisp", "localization", "LXDE", "LxQt", "LXQt", "MATE", "mail", "math", "misc",
    "net", "news", "ocaml", "oldlibs", "otherosfs", "perl", "php", "python", "ruby",
    "science", "shells", "sound", "tex", "text", "Trinity", "utils", "vcs", "video",
    "virtual", "web", "x11", "xfce", "zope",
    // Modern additions used across the tree.
    "ros", "COSMIC",
];

fn is_known_arch(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    KNOWN_ARCHES.iter().any(|a| *a == n) || ARCH_GROUPS.iter().any(|g| *g == n)
}

fn is_known_section(s: &str) -> bool {
    if KNOWN_SECTIONS.contains(&s) {
        return true;
    }
    s.strip_prefix("non-free/")
        .or_else(|| s.strip_prefix("contrib/"))
        .is_some_and(|rest| KNOWN_SECTIONS.contains(&rest))
}

/// Evaluate the final scalar value of `name` as assigned by `stmts` (in
/// order, handling `+=`), against the initial `context`. Returns the span of
/// the last assignment and the evaluated value.
fn eval_var(stmts: &[Stmt], context: &Context, name: &str) -> Option<(Span, String)> {
    let mut ctx = context.clone();
    let mut runner = |_stages: &[Vec<String>]| Ok(String::new());
    let mut warnings = Vec::new();
    let mut result: Option<(Span, String)> = None;
    for stmt in stmts {
        if stmt.name != name {
            continue;
        }
        let ValueExpr::Scalar(w) = &stmt.value else {
            continue;
        };
        let s = eval::eval_scalar_word(w, &mut ctx, &mut runner, &mut warnings, false).ok()?;
        let value = match stmt.op {
            AssignOp::Eq => s,
            AssignOp::PlusEq => {
                let mut v = result.take().map(|(_, v)| v).unwrap_or_default();
                v.push_str(&s);
                v
            }
        };
        result = Some((stmt.span, value));
    }
    result
}

/// Whether `name` looks like a genuine ABBS variable: a known base name or a
/// `base__ARCH` variant of one.
fn is_known_name(name: &str) -> bool {
    if FRAMEWORK_VARS.contains(&name) {
        return true;
    }
    KNOWN_BASES.iter().any(|b| {
        name == *b
            || (name.starts_with(b)
                && name.as_bytes().get(b.len()) == Some(&b'_')
                && name.as_bytes().get(b.len() + 1) == Some(&b'_'))
    })
}

/// `typo`: an undefined variable whose name is within one edit of exactly
/// one defined or framework variable.
///
/// The fix direction is decided by which side is a known ABBS name:
///
/// * reference is the typo (`BUIDDEP__NOJAVA` vs defined `BUILDDEP__NOJAVA`)
///   → rename every reference of the typo;
/// * definition is the typo (`BUILDEP__RETRO` vs referenced
///   `BUILDDEP__RETRO`) → rename the definition.
fn rule_typo(c: &str, stmts: &[Stmt], context: &Context) -> Vec<Lint> {
    let defined = undefined::collect_defined(stmts, context);
    let refs = undefined::collect_refs(stmts);

    // Group references by name so each typo can be reported/fixed at every
    // occurrence.
    let mut refs_by_name: HashMap<&str, Vec<&VarRef>> = HashMap::new();
    for r in &refs {
        refs_by_name.entry(r.name.as_str()).or_default().push(r);
    }

    let mut out = Vec::new();
    for (&x, occurrences) in &refs_by_name {
        if defined.contains(x) || FRAMEWORK_VARS.contains(&x) {
            continue;
        }

        // Candidate names: everything defined in this file plus the known
        // framework variables.
        let candidates = defined
            .iter()
            .map(|s| s.as_str())
            .chain(FRAMEWORK_VARS.iter().copied());
        let mut matches: Vec<&str> = Vec::new();
        for known in candidates {
            if edit_distance(x, known) <= 1 {
                matches.push(known);
            }
        }
        // Only act when the match is unambiguous.
        if matches.len() != 1 {
            continue;
        }
        let y = matches[0];

        let known_x = is_known_name(x);
        let known_y = is_known_name(y);
        match (known_x, known_y) {
            // The reference is the typo: rename every reference X -> Y.
            (false, true) => {
                for r in occurrences {
                    let (span, fix) = match r.name_start {
                        Some(ns) => (
                            byte_span(c, ns, ns + r.name.len()),
                            Some(LintFix {
                                start: ns,
                                end: ns + r.name.len(),
                                replacement: y.to_string(),
                            }),
                        ),
                        None => (r.span.into(), None),
                    };
                    out.push(Lint {
                        rule: "typo",
                        severity: LintSeverity::Warning,
                        span,
                        message: format!("variable `{x}` never defined — did you mean `{y}`?"),
                        fix,
                    });
                }
            }
            // The definition is the typo: rename the definition Y -> X.
            (true, false) => {
                if let Some(stmt) = stmts.iter().find(|s| s.name == y) {
                    let ns = stmt.span.byte;
                    out.push(Lint {
                        rule: "typo",
                        severity: LintSeverity::Warning,
                        span: byte_span(c, ns, ns + y.len()),
                        message: format!(
                            "`{y}` is defined but never referenced — did you mean `{x}`?"
                        ),
                        fix: Some(LintFix {
                            start: ns,
                            end: ns + y.len(),
                            replacement: x.to_string(),
                        }),
                    });
                }
            }
            // Both (or neither) are known ABBS names: ambiguous, no autofix.
            _ => {}
        }
    }
    out
}

/// `pkgdes-style`: PKGDES should start with an uppercase letter or a digit
/// (e.g. `3D visualization tool ...`) and not end with punctuation
/// (package-styling-manual §2.3).
fn rule_pkgdes_style(stmts: &[Stmt], context: &Context) -> Vec<Lint> {
    let Some((span, value)) = eval_var(stmts, context, "PKGDES") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let first = value.chars().next().unwrap_or(' ');
    if !(first.is_ascii_uppercase() || first.is_ascii_digit()) {
        out.push(Lint {
            rule: "pkgdes-style",
            severity: LintSeverity::Warning,
            span: span.into(),
            message: format!(
                "PKGDES should start with an uppercase letter or a digit (starts with `{first}`)"
            ),
            fix: None,
        });
    }
    if let Some(last) = value.chars().last() {
        if ".,;:!?".contains(last) {
            out.push(Lint {
                rule: "pkgdes-style",
                severity: LintSeverity::Warning,
                span: span.into(),
                message: format!("PKGDES should not end with punctuation (`{last}`)"),
                fix: None,
            });
        }
    }
    out
}

/// `fail-arch`: FAIL_ARCH must be an extglob expression `@(arch|...)` /
/// `!(arch|...)` over known architectures and groups.
fn rule_fail_arch(stmts: &[Stmt], context: &Context) -> Vec<Lint> {
    let Some((span, value)) = eval_var(stmts, context, "FAIL_ARCH") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let value = value.trim();
    // Empty (e.g. a dynamic `FAIL_ARCH="${__CROSS}"` that did not expand in
    // this context) cannot be validated.
    if value.is_empty() {
        return Vec::new();
    }
    let inner = value
        .strip_prefix("@(")
        .or_else(|| value.strip_prefix("!("))
        .and_then(|s| s.strip_suffix(')'));
    let Some(inner) = inner else {
        out.push(Lint {
            rule: "fail-arch",
            severity: LintSeverity::Warning,
            span: span.into(),
            message: format!(
                "FAIL_ARCH should be an extglob expression `@(arch|...)` or `!(arch|...)`, got `{value}`"
            ),
            fix: None,
        });
        return out;
    };
    for part in inner.split('|') {
        let part = part.trim();
        if part.is_empty() || is_known_arch(part) {
            continue;
        }
        out.push(Lint {
            rule: "fail-arch",
            severity: LintSeverity::Warning,
            span: span.into(),
            message: format!("FAIL_ARCH contains unknown architecture or group `{part}`"),
            fix: None,
        });
    }
    out
}

/// `srctbl-http`: SRCTBL should use HTTPS rather than HTTP or FTP
/// (QA W112 covers the `http://` case).
fn rule_srctbl_http(stmts: &[Stmt], context: &Context) -> Vec<Lint> {
    let Some((span, value)) = eval_var(stmts, context, "SRCTBL") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if value.starts_with("http://") {
        out.push(Lint {
            rule: "srctbl-http",
            severity: LintSeverity::Warning,
            span: span.into(),
            message: "SRCTBL uses insecure http:// (QA W112)".to_string(),
            fix: None,
        });
    } else if value.starts_with("ftp://") {
        out.push(Lint {
            rule: "srctbl-http",
            severity: LintSeverity::Warning,
            span: span.into(),
            message: "SRCTBL uses insecure ftp://".to_string(),
            fix: None,
        });
    }
    out
}

/// `pkgsection`: PKGSEC must be a canonical section (autobuild
/// `sets/section`); a close match is suggested when it is unique.
fn rule_pkgsection(stmts: &[Stmt], context: &Context) -> Vec<Lint> {
    let Some((span, value)) = eval_var(stmts, context, "PKGSEC") else {
        return Vec::new();
    };
    let value = value.trim();
    if value.is_empty() || is_known_section(value) {
        return Vec::new();
    }
    // Suggest the closest canonical section, but only when the closest
    // distance is unique (e.g. `util` → `utils`, not the tie with `mail`).
    let mut best: Option<(usize, &str)> = None;
    let mut ambiguous = false;
    for s in KNOWN_SECTIONS {
        let d = edit_distance(value, s);
        match best {
            None => best = Some((d, s)),
            Some((bd, _)) if d < bd => {
                best = Some((d, s));
                ambiguous = false;
            }
            Some((bd, _)) if d == bd => ambiguous = true,
            Some(_) => {}
        }
    }
    let mut message = format!("PKGSEC `{value}` is not a canonical section");
    if let Some((d, sugg)) = best {
        if !ambiguous && d <= 2 {
            message.push_str(&format!(" — did you mean `{sugg}`?"));
        }
    }
    vec![Lint {
        rule: "pkgsection",
        severity: LintSeverity::Warning,
        span: span.into(),
        message,
        fix: None,
    }]
}

/// `required-fields`: the ACBS spec format requires `PKGNAME` / `PKGSEC` /
/// `PKGDES` in a `defines` file, and `VER` plus one of `SRCS` / `DUMMYSRC`
/// in a `spec` file. The file type is inferred from which family of
/// variables it assigns (a `defines` sets the PKG* metadata, a `spec` sets
/// `VER` / `SRCS`). Reported at the top of the file.
fn rule_required_fields(c: &str, stmts: &[Stmt]) -> Vec<Lint> {
    let assigned: HashSet<&str> = stmts.iter().map(|s| s.name.as_str()).collect();
    let mut out = Vec::new();
    let top = byte_span(c, 0, 0);

    let is_defines = ["PKGNAME", "PKGSEC", "PKGDES"].iter().any(|v| assigned.contains(v));
    if is_defines {
        for required in ["PKGNAME", "PKGSEC", "PKGDES"] {
            if !assigned.contains(required) {
                out.push(Lint {
                    rule: "required-fields",
                    severity: LintSeverity::Warning,
                    span: top,
                    message: format!("defines is missing required {required}"),
                    fix: None,
                });
            }
        }
    }

    let is_spec = ["VER", "SRCS", "DUMMYSRC"].iter().any(|v| assigned.contains(v));
    if is_spec {
        if !assigned.contains("VER") {
            out.push(Lint {
                rule: "required-fields",
                severity: LintSeverity::Warning,
                span: top,
                message: "spec is missing required VER".to_string(),
                fix: None,
            });
        }
        // Plain `SRCS`, arch-specific `SRCS__<ARCH>` variants, or `DUMMYSRC`
        // all satisfy the source requirement.
        let has_src = assigned.contains("SRCS")
            || assigned.contains("DUMMYSRC")
            || assigned.iter().any(|v| v.starts_with("SRCS__"));
        if !has_src {
            out.push(Lint {
                rule: "required-fields",
                severity: LintSeverity::Warning,
                span: top,
                message: "spec should define one of SRCS / SRCS__ARCH / DUMMYSRC".to_string(),
                fix: None,
            });
        }
    }
    out
}

/// Build a [`DiagnosticSpan`] for a byte range, computing line/column from
/// the source.
fn byte_span(src: &str, start: usize, end: usize) -> DiagnosticSpan {
    let (line, col) = line_col(src, start);
    DiagnosticSpan {
        line,
        col,
        byte: end,
        prev_byte: start,
    }
}

/// 1-based `(line, column)` of a byte offset in `src`.
fn line_col(src: &str, byte: usize) -> (usize, usize) {
    let byte = byte.min(src.len());
    let mut line = 1;
    let mut last_nl = 0;
    for (i, b) in src.bytes().enumerate() {
        if i >= byte {
            break;
        }
        if b == b'\n' {
            line += 1;
            last_nl = i + 1;
        }
    }
    (line, byte - last_nl + 1)
}

/// Classic Levenshtein edit distance (case-sensitive).
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut cur = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        cur[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            cur[j + 1] = if ca == cb {
                prev[j]
            } else {
                1 + prev[j].min(prev[j + 1]).min(cur[j])
            };
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[b.len()]
}
