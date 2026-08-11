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
mod parser;
mod substitution;
mod value;
mod variables;

use eval::{eval_stmts, Runner};
use std::collections::HashMap;

pub use error::{ParseError, ParseErrorInfo};
pub use value::Value;

/// The variable context: name -> value.
pub type Context = HashMap<String, Value>;

/// Parse a `spec` / `defines` file and apply its variable assignments to
/// `context`.
///
/// Command substitutions (`$( ... )`) are **never executed**: they expand to
/// an empty string, like an undefined variable. This keeps the parser safe
/// to run on untrusted files. If you explicitly need real command output,
/// use [`parse_with_runner`] (at your own risk).
pub fn parse(c: &str, context: &mut Context) -> Result<(), Vec<ParseError>> {
    parse_with_runner(c, context, &mut |_stages: &[Vec<String>]| {
        Ok(String::new())
    })
}

/// Parse a `spec` / `defines` file and apply its variable assignments to
/// `context`, using `runner` to evaluate `$( ... )` command substitutions.
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
pub fn parse_with_runner(
    c: &str,
    context: &mut Context,
    runner: &mut Runner,
) -> Result<(), Vec<ParseError>> {
    let stmts = match parser::parse_program(c) {
        Ok(stmts) => stmts,
        Err(errors) => return Err(errors),
    };

    let mut errors = Vec::new();
    for stmt in &stmts {
        if let Err(e) = eval_stmts(std::slice::from_ref(stmt), context, runner) {
            let span = stmt.span;
            errors.push(ParseError {
                line: span.line,
                col: span.col,
                byte: span.byte,
                prev_byte: span.byte,
                error: e,
            });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
