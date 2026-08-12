//! The ACBS Package Metadata Language parser.
//!
//! This is a self-contained parser for the Bash subset used by AOSC OS
//! `spec` and `defines` files. It does not depend on a general-purpose Bash
//! parser; only variable assignments, comments and blank lines are allowed
//! at the top level, together with Bash's variable expansion syntax.

mod ast;
mod error;
mod eval;
mod glob;
mod lexer;
mod lint;
mod parser;
mod substitution;
mod undefined;
mod value;

pub use lint::{lint, Lint, LintFix, LintSeverity};

use eval::{eval_stmts, Runner};
use std::collections::HashMap;

pub use error::{Diagnostic, DiagnosticInfo, DiagnosticSpan, ParseErrorInfo};
pub use value::Value;

/// The variable context: name -> value.
pub type Context = HashMap<String, Value>;

/// The outcome of parsing a `spec` / `defines` file.
///
/// Parsing always produces a full report: fatal [`Diagnostic`]s (errors)
/// that stopped some assignments from being applied, and non-fatal ones
/// (warnings, e.g. a `$( ... )` command substitution that expanded to an
/// empty string because it is never executed). Use [`ParseResult::is_ok`] to
/// check whether parsing fully succeeded.
#[derive(Debug, Default)]
pub struct ParseResult {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

impl ParseResult {
    /// Whether parsing succeeded without errors (warnings are not fatal).
    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// Whether parsing produced any errors.
    pub fn is_err(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Parse a `spec` / `defines` file and apply its variable assignments to
/// `context`, returning a full [`ParseResult`] with any errors and warnings.
///
/// Command substitutions (`$( ... )`) are **never executed**: they expand to
/// an empty string, like an undefined variable, and are reported as a
/// [`Diagnostic`]. This keeps the parser safe to run on untrusted files.
/// If you explicitly need real command output, use [`parse_with_runner`] (at
/// your own risk).
pub fn parse(c: &str, context: &mut Context) -> ParseResult {
    // The default runner never executes commands, so an empty `$( ... )`
    // means a command was skipped — surface that as a warning.
    parse_impl(
        c,
        context,
        &mut |_stages: &[Vec<String>]| Ok(String::new()),
        true,
    )
}

/// Parse a `spec` / `defines` file and apply its variable assignments to
/// `context`, using `runner` to evaluate `$( ... )` command substitutions,
/// returning a full [`ParseResult`] with any errors and warnings.
///
/// # Security
///
/// The runner **executes arbitrary commands** found in the file. Only use
/// this on files you trust, with a runner that sandboxes or restricts
/// execution; the default [`parse`] never executes anything.
///
/// The runner receives the expanded pipeline (each stage is a list of
/// command words) and must return the command's standard output (trailing
/// newline stripped), or an error.
pub fn parse_with_runner(c: &str, context: &mut Context, runner: &mut Runner) -> ParseResult {
    // A real runner executes commands; an empty result is a legitimate
    // command output, so no "skipped command" warnings are emitted here.
    parse_impl(c, context, runner, false)
}

fn parse_impl(
    c: &str,
    context: &mut Context,
    runner: &mut Runner,
    warn_skipped_command: bool,
) -> ParseResult {
    let stmts = match parser::parse_program(c) {
        Ok(stmts) => stmts,
        Err(errors) => {
            return ParseResult {
                errors,
                warnings: Vec::new(),
            }
        }
    };

    let mut result = ParseResult::default();
    for stmt in &stmts {
        if let Err(e) = eval_stmts(
            std::slice::from_ref(stmt),
            context,
            runner,
            &mut result.warnings,
            warn_skipped_command,
        ) {
            let span = stmt.span;
            result.errors.push(Diagnostic {
                span: span.into(),
                info: DiagnosticInfo::Error(e),
            });
        }
    }

    // Static check, independent of the runner: references to variables that
    // are never defined (neither in the initial context nor by any
    // assignment in this file). Bash expands these to the empty string, so
    // they are warnings — but they usually indicate a package bug.
    for (span, name) in undefined::collect_undefined_vars(&stmts, context) {
        result.warnings.push(Diagnostic {
            span: span.into(),
            info: DiagnosticInfo::Warning(format!(
                "variable `{name}` referenced but never defined"
            )),
        });
    }

    result
}
