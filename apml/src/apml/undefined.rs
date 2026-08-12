//! Static analysis over the parsed AST: variable definitions and references.
//!
//! The main consumer is the `undefined_variable` lint rule (a variable that
//! is *referenced* but never *defined* — neither in the initial context nor
//! by any assignment in the file). Bash expands such variables to the empty
//! string (or `0` in arithmetic), so they are surfaced as warnings rather
//! than errors; they usually indicate a package bug such as a typo or a
//! missing definition.
//!
//! This module also exposes the raw reference/definition collections so the
//! lint rules can implement `unused_variable` and typo detection on top of
//! them.

use super::ast::*;
use super::eval::Context;
use std::collections::HashSet;

/// A single variable reference in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VarRef {
    /// Span of the enclosing word (for reporting line/column).
    pub span: Span,
    /// The referenced variable name.
    pub name: String,
    /// Byte offset of the name in the source, when known (`None` for
    /// references inside `$(( ... ))`, whose exact position is not tracked).
    pub name_start: Option<usize>,
}

/// Every variable name that is *defined*: the initial `context` plus any
/// assignment in `stmts` (including `+=` and `${name:=word}`).
pub(crate) fn collect_defined(stmts: &[Stmt], context: &Context) -> HashSet<String> {
    let mut defined: HashSet<String> = context.keys().cloned().collect();
    for stmt in stmts {
        defined.insert(stmt.name.clone());
        collect_assigned_in_value(&stmt.value, &mut defined);
    }
    defined
}

/// Every variable *reference* in `stmts` (excluding the words inside
/// `$( ... )` command substitutions, which are shell internals).
pub(crate) fn collect_refs(stmts: &[Stmt]) -> Vec<VarRef> {
    let mut refs = Vec::new();
    for stmt in stmts {
        collect_refs_in_value(&stmt.value, &mut refs);
    }
    refs
}

/// Words that are reserved in Bash and must not be treated as variable
/// references when scanning `$(( ... ))` arithmetic content.
const ARITH_RESERVED: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "while", "until", "do", "done", "in", "case",
    "esac", "function", "select", "time", "return", "break", "continue",
];

/// Variables provided by the ABBS/autobuild framework or the build
/// environment rather than by `spec` / `defines` files themselves.
/// Referencing these is normal, so they must not be reported as
/// "never defined".
pub(crate) const FRAMEWORK_VARS: &[&str] = &[
    // Autobuild-provided per-package metadata.
    "SRCDIR", "PKGDIR", "PKGVER", "PKGREL", "ARCH",
    // Standard ABBS spec variables (often referenced with a default,
    // e.g. `${REL:-0}`).
    "VER", "REL",
    // Autobuild-provided helper variables.
    "ABPY3VER",
    // Common build-environment / configure variables.
    "CFLAGS", "CXXFLAGS", "CPPFLAGS", "LDFLAGS", "LIBS", "PREFIX", "LIBDIR",
    "BINDIR", "SBINDIR", "INCLUDEDIR", "DATADIR", "SYSCONFDIR",
    "LOCALSTATEDIR", "MANDIR", "INFODIR", "CMAKE_INSTALL_PREFIX", "DESTDIR",
    // Common environment variables.
    "PATH", "HOME", "USER", "LOGNAME", "SHELL", "PWD", "OLDPWD", "TERM",
    "LANG", "LC_ALL", "LC_CTYPE", "TMPDIR", "HOSTNAME", "UID", "EUID",
];

/// Collect variable names that are *referenced* somewhere in `stmts` but
/// never *defined* — neither in the initial `context` nor by any assignment
/// in the file.
///
/// Bash expands such variables to the empty string (or `0` in arithmetic),
/// so this is not an error; the warning surfaces likely package bugs such as
/// a typo or a missing definition (e.g. `BUILDDEP="${PKGSUG} ..."` with
/// `PKGSUG` never defined).
///
/// Returns one `(span, name)` pair per statement that references the name,
/// so each location can be reported.
pub(crate) fn collect_undefined_vars(stmts: &[Stmt], context: &Context) -> Vec<(Span, String)> {
    let defined = collect_defined(stmts, context);

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for r in collect_refs(stmts) {
        if !defined.contains(&r.name)
            && !FRAMEWORK_VARS.contains(&r.name.as_str())
            && seen.insert(r.name.clone())
        {
            out.push((r.span, r.name));
        }
    }
    out
}

/// Collect names assigned by `${name:=word}` inside a value (the only
/// value-position assignment form).
fn collect_assigned_in_value(value: &ValueExpr, defined: &mut HashSet<String>) {
    match value {
        ValueExpr::Scalar(w) => collect_assigned_in_word(w, defined),
        ValueExpr::Array(ws) => {
            for w in ws {
                collect_assigned_in_word(w, defined);
            }
        }
    }
}

fn collect_assigned_in_word(w: &Word, defined: &mut HashSet<String>) {
    for f in &w.fields {
        if let Field::Param(Param::Braced {
            name,
            op: BracedOp::Assign { .. },
            ..
        }) = f
        {
            defined.insert(name.clone());
        }
    }
}

/// Collect every variable reference inside a value.
fn collect_refs_in_value(value: &ValueExpr, refs: &mut Vec<VarRef>) {
    match value {
        ValueExpr::Scalar(w) => collect_refs_in_word(w, refs),
        ValueExpr::Array(ws) => {
            for w in ws {
                collect_refs_in_word(w, refs);
            }
        }
    }
}

fn collect_refs_in_word(w: &Word, refs: &mut Vec<VarRef>) {
    for f in &w.fields {
        collect_ref_in_field(f, w.span, refs);
        if let Field::Arith(s) = f {
            for name in arith_identifiers(s) {
                refs.push(VarRef {
                    span: w.span,
                    name,
                    name_start: None,
                });
            }
        }
    }
}

fn collect_ref_in_field(f: &Field, span: Span, refs: &mut Vec<VarRef>) {
    match f {
        Field::Param(Param::Plain { name, name_start })
        | Field::Param(Param::Braced { name, name_start, .. })
        | Field::Param(Param::Length { name, name_start, .. }) => refs.push(VarRef {
            span,
            name: name.clone(),
            name_start: Some(*name_start),
        }),
        Field::DoubleQuoted(fs) => {
            for g in fs {
                collect_ref_in_field(g, span, refs);
            }
        }
        // Literal / Escaped / SingleQuoted / Command: the command's words
        // are shell internals, not metadata variables — skip them.
        _ => {}
    }
}

/// Extract variable names from `$(( ... ))` content. Bare identifiers are
/// variable references in arithmetic; `$NAME` works too (the `$` is
/// skipped). Reserved words are ignored.
fn arith_identifiers(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = s.chars().peekable();
    while let Some(&c) = it.peek() {
        if c.is_ascii_alphabetic() || c == '_' {
            let mut name = String::new();
            while let Some(&c) = it.peek() {
                if c.is_ascii_alphanumeric() || c == '_' {
                    name.push(c);
                    it.next();
                } else {
                    break;
                }
            }
            if !ARITH_RESERVED.contains(&name.as_str()) {
                out.push(name);
            }
        } else {
            it.next();
        }
    }
    out
}
