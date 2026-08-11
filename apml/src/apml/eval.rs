//! Evaluation of parsed statements against a [`Context`].
//!
//! Semantics follow Bash for the supported subset: undefined variables
//! expand to an empty string, arrays support `"${arr[@]}"` expansion, and
//! `VAR+="x"` / `VAR+=(...)` append to scalar / array values.

use super::ast::*;
use super::error::{Diagnostic, DiagnosticInfo, ParseErrorInfo};
use super::substitution;
use super::value::Value;
use std::borrow::Cow;
use std::collections::HashMap;

/// The variable context: name -> value.
pub type Context = HashMap<String, Value>;

/// A runner for `$( ... )` command substitutions. Receives the expanded
/// pipeline (each stage is a list of command words) and returns the
/// command's standard output (without the trailing newline), or an error.
pub type Runner<'a> = dyn FnMut(&[Vec<String>]) -> Result<String, ParseErrorInfo> + 'a;

pub fn eval_stmts(
    stmts: &[Stmt],
    context: &mut Context,
    runner: &mut Runner,
    warnings: &mut Vec<Diagnostic>,
    warn_skipped_command: bool,
) -> Result<(), ParseErrorInfo> {
    for stmt in stmts {
        eval_stmt(stmt, context, runner, warnings, warn_skipped_command)?;
    }
    Ok(())
}

fn eval_stmt(
    stmt: &Stmt,
    ctx: &mut Context,
    runner: &mut Runner,
    warnings: &mut Vec<Diagnostic>,
    warn_skipped_command: bool,
) -> Result<(), ParseErrorInfo> {
    let name = &stmt.name;
    match &stmt.value {
        ValueExpr::Scalar(w) => {
            let s = eval_scalar_word(w, ctx, runner, warnings, warn_skipped_command)?;
            match stmt.op {
                AssignOp::Eq => {
                    ctx.insert(name.clone(), Value::Scalar(s));
                }
                AssignOp::PlusEq => append_scalar(ctx, name, &s),
            }
        }
        ValueExpr::Array(words) => {
            let mut elems = Vec::new();
            for w in words {
                elems.extend(eval_array_word(
                    w,
                    ctx,
                    runner,
                    warnings,
                    warn_skipped_command,
                )?);
            }
            match stmt.op {
                AssignOp::Eq => {
                    ctx.insert(name.clone(), Value::Array(elems));
                }
                AssignOp::PlusEq => append_array(ctx, name, elems),
            }
        }
    }
    Ok(())
}

fn append_scalar(ctx: &mut Context, name: &str, s: &str) {
    match ctx.get_mut(name) {
        None => {
            ctx.insert(name.to_string(), Value::Scalar(s.to_string()));
        }
        Some(Value::Scalar(old)) => old.push_str(s),
        Some(Value::Array(arr)) => arr.push(s.to_string()),
    }
}

fn append_array(ctx: &mut Context, name: &str, elems: Vec<String>) {
    match ctx.remove(name) {
        None => {
            ctx.insert(name.to_string(), Value::Array(elems));
        }
        Some(Value::Array(mut arr)) => {
            arr.extend(elems);
            ctx.insert(name.to_string(), Value::Array(arr));
        }
        Some(Value::Scalar(old)) => {
            let mut arr = vec![old];
            arr.extend(elems);
            ctx.insert(name.to_string(), Value::Array(arr));
        }
    }
}

/// Evaluate a word in a scalar context: the concatenation of its fields.
pub fn eval_scalar_word(
    w: &Word,
    ctx: &mut Context,
    runner: &mut Runner,
    warnings: &mut Vec<Diagnostic>,
    warn_skipped_command: bool,
) -> Result<String, ParseErrorInfo> {
    eval_fields_scalar(
        &w.fields,
        w.span,
        ctx,
        runner,
        warnings,
        warn_skipped_command,
    )
}

fn eval_fields_scalar(
    fields: &[Field],
    span: Span,
    ctx: &mut Context,
    runner: &mut Runner,
    warnings: &mut Vec<Diagnostic>,
    warn_skipped_command: bool,
) -> Result<String, ParseErrorInfo> {
    let mut out = String::new();
    for f in fields {
        match f {
            Field::Literal(s) => out.push_str(s),
            Field::Escaped(c) => out.push(*c),
            Field::SingleQuoted(s) => out.push_str(s),
            Field::DoubleQuoted(fs) => out.push_str(&eval_fields_scalar(
                fs,
                span,
                ctx,
                runner,
                warnings,
                warn_skipped_command,
            )?),
            Field::Param(p) => out.push_str(&eval_param_scalar(
                p,
                ctx,
                runner,
                warnings,
                warn_skipped_command,
            )?),
            Field::Command(cmds) => {
                let mut output = String::new();
                for cmd in cmds {
                    let mut stages = Vec::new();
                    for stage_words in &cmd.stages {
                        let mut words = Vec::new();
                        for w in stage_words {
                            words.push(eval_scalar_word(
                                w,
                                ctx,
                                runner,
                                warnings,
                                warn_skipped_command,
                            )?);
                        }
                        stages.push(words);
                    }
                    let s = runner(&stages)?;
                    output.push_str(&s);
                }
                if warn_skipped_command && output.is_empty() {
                    // The default runner never executes `$( ... )`, so an
                    // empty result hides a skipped command. With a real
                    // runner, empty output is a legitimate command result
                    // and is not reported.
                    warnings.push(Diagnostic {
                        span: span.into(),
                        info: DiagnosticInfo::Warning(
                            "command substitution `$(...)` expanded to an empty string (not executed)"
                                .to_string(),
                        ),
                    });
                }
                out.push_str(&output);
            }
            Field::Arith(s) => out.push_str(&eval_arith(s, ctx)?),
        }
    }
    Ok(out)
}

/// Evaluate a word in an array-element context: may expand to multiple
/// values.
///
/// Word splitting happens only at whitespace introduced by *unquoted*
/// parameter/command/arithmetic expansions; quoted or escaped whitespace is
/// kept as part of a single element (Bash semantics).
fn eval_array_word(
    w: &Word,
    ctx: &mut Context,
    runner: &mut Runner,
    warnings: &mut Vec<Diagnostic>,
    warn_skipped_command: bool,
) -> Result<Vec<String>, ParseErrorInfo> {
    let mut values: Vec<String> = Vec::new();
    for f in &w.fields {
        match f {
            Field::Literal(s) | Field::SingleQuoted(s) => append_or_push(&mut values, s),
            Field::Escaped(c) => append_char(&mut values, *c),
            Field::DoubleQuoted(fs) => {
                // `"${arr[@]}"` / `"${arr[@]<op>...}"` / `"$@"` alone →
                // one value per array element.
                if w.fields.len() == 1 && fs.len() == 1 {
                    if let Field::Param(Param::Braced {
                        name,
                        index: Some(Index::At),
                        op,
                    }) = &fs[0]
                    {
                        return apply_array_op(
                            name,
                            op,
                            ctx,
                            runner,
                            warnings,
                            warn_skipped_command,
                        );
                    }
                    if let Field::Param(Param::Special('@')) = &fs[0] {
                        return Ok(Vec::new());
                    }
                }
                // Inside double quotes: no word splitting.
                let s =
                    eval_fields_scalar(fs, w.span, ctx, runner, warnings, warn_skipped_command)?;
                append_or_push(&mut values, &s);
            }
            Field::Param(p) => {
                // Unquoted `${arr[@]<op>...}` / `$@` alone.
                if w.fields.len() == 1 {
                    if let Param::Braced {
                        name,
                        index: Some(Index::At),
                        op,
                    } = p
                    {
                        let mut out = Vec::new();
                        for e in
                            apply_array_op(name, op, ctx, runner, warnings, warn_skipped_command)?
                        {
                            out.extend(ifs_split(&e));
                        }
                        return Ok(out);
                    }
                    if let Param::Special('@') = p {
                        return Ok(Vec::new());
                    }
                }
                // Unquoted expansion: word-split the result.
                let s = eval_param_scalar(p, ctx, runner, warnings, warn_skipped_command)?;
                split_append(&mut values, &s);
            }
            Field::Command(_) | Field::Arith(_) => {
                let single = Word {
                    span: w.span,
                    fields: vec![f.clone()],
                };
                let s = eval_scalar_word(&single, ctx, runner, warnings, warn_skipped_command)?;
                split_append(&mut values, &s);
            }
        }
    }
    Ok(values)
}

/// Apply a braced operation to every element of an array
/// (`${arr[@]<op>...}`).
fn apply_array_op(
    name: &str,
    op: &BracedOp,
    ctx: &mut Context,
    runner: &mut Runner,
    warnings: &mut Vec<Diagnostic>,
    warn_skipped_command: bool,
) -> Result<Vec<String>, ParseErrorInfo> {
    let elems = array_elems(name, ctx);
    if elems.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for e in elems {
        // Fast paths move the element (or produce empty) with no allocation;
        // only the computed ops need the full evaluator.
        match braced_fast(op, true, e.is_empty()) {
            BracedFast::SelfOrigin => out.push(e),
            BracedFast::Empty => out.push(String::new()),
            BracedFast::Computed => {
                out.push(eval_braced_with_origin(
                    name,
                    &e,
                    op,
                    ctx,
                    runner,
                    warnings,
                    warn_skipped_command,
                )?);
            }
        }
    }
    Ok(out)
}

fn append_or_push(values: &mut Vec<String>, s: &str) {
    if values.is_empty() {
        values.push(String::new());
    }
    values.last_mut().unwrap().push_str(s);
}

/// Append a single char to the last element (or a new one), without
/// allocating an intermediate `String`.
fn append_char(values: &mut Vec<String>, c: char) {
    if values.is_empty() {
        values.push(c.to_string());
    } else {
        values.last_mut().unwrap().push(c);
    }
}

fn split_append(values: &mut Vec<String>, s: &str) {
    let mut parts = ifs_split(s);
    if parts.is_empty() {
        return;
    }
    let first = parts.remove(0);
    append_or_push(values, &first);
    values.extend(parts);
}

fn array_elems(name: &str, ctx: &Context) -> Vec<String> {
    match ctx.get(name) {
        Some(Value::Array(a)) => a.clone(),
        _ => Vec::new(),
    }
}

/// Default IFS word splitting (space / tab / newline).
fn ifs_split(s: &str) -> Vec<String> {
    s.split([' ', '\t', '\n'])
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect()
}

fn eval_param_scalar<'a>(
    p: &Param,
    ctx: &'a mut Context,
    runner: &mut Runner,
    warnings: &mut Vec<Diagnostic>,
    warn_skipped_command: bool,
) -> Result<Cow<'a, str>, ParseErrorInfo> {
    match p {
        Param::Plain(name) => Ok(param_value(name, None, ctx)),
        Param::Special(_) | Param::Positional(_) => Ok(Cow::Borrowed("")),
        Param::Length { name, index } => match index {
            Some(Index::At) | Some(Index::Star) => {
                Ok(Cow::Owned(format!("{}", array_elems(name, ctx).len())))
            }
            _ => Ok(Cow::Owned(format!(
                "{}",
                param_value(name, None, ctx).chars().count()
            ))),
        },
        Param::Braced { name, index, op } => {
            eval_braced(name, index, op, ctx, runner, warnings, warn_skipped_command)
        }
    }
}

/// The value of a parameter; undefined variables expand to an empty string
/// (Bash semantics). Referencing an array without a subscript yields its
/// first element. Borrows the stored value when possible instead of
/// allocating a copy.
fn param_value<'a>(name: &str, index: Option<&Index>, ctx: &'a Context) -> Cow<'a, str> {
    match ctx.get(name) {
        None => Cow::Borrowed(""),
        Some(Value::Scalar(s)) => Cow::Borrowed(s.as_str()),
        Some(Value::Array(a)) => match index {
            None | Some(Index::Number(0)) => Cow::Borrowed(a.first().map_or("", String::as_str)),
            Some(Index::Number(i)) => Cow::Borrowed(a.get(*i).map_or("", String::as_str)),
            Some(Index::At) | Some(Index::Star) => Cow::Owned(a.join(" ")),
        },
    }
}

/// The outcome of a braced operation when only the origin and its state are
/// consulted — used to short-circuit with zero copying.
enum BracedFast {
    /// The result is the origin value itself.
    SelfOrigin,
    /// The result is the empty string.
    Empty,
    /// The result needs word evaluation and/or context mutation.
    Computed,
}

/// Decide what a `${...}` does with its origin without touching the context.
fn braced_fast(op: &BracedOp, is_set: bool, origin_empty: bool) -> BracedFast {
    match op {
        BracedOp::Value => BracedFast::SelfOrigin,
        BracedOp::Default { colon, .. } => {
            let use_self = if *colon { is_set && !origin_empty } else { is_set };
            if use_self {
                BracedFast::SelfOrigin
            } else {
                BracedFast::Computed
            }
        }
        BracedOp::Alternative { colon, .. } => {
            let use_alt = if *colon { is_set && !origin_empty } else { is_set };
            if use_alt {
                BracedFast::Computed
            } else {
                BracedFast::Empty
            }
        }
        BracedOp::Error { colon, .. } => {
            let is_err = if *colon { !is_set || origin_empty } else { !is_set };
            if is_err {
                BracedFast::Computed
            } else {
                BracedFast::SelfOrigin
            }
        }
        BracedOp::Assign { colon, .. } => {
            let need_assign = if *colon { !is_set || origin_empty } else { !is_set };
            if need_assign {
                BracedFast::Computed
            } else {
                BracedFast::SelfOrigin
            }
        }
        _ => BracedFast::Computed,
    }
}

fn eval_braced<'a>(
    name: &str,
    index: &Option<Index>,
    op: &BracedOp,
    ctx: &'a mut Context,
    runner: &mut Runner,
    warnings: &mut Vec<Diagnostic>,
    warn_skipped_command: bool,
) -> Result<Cow<'a, str>, ParseErrorInfo> {
    let is_set = ctx.contains_key(name);

    // Decide with a short-lived borrow so `ctx` is free again afterwards.
    let fast = {
        let origin = param_value(name, index.as_ref(), ctx);
        braced_fast(op, is_set, origin.is_empty())
    };

    match fast {
        // Zero-copy: the result is the origin itself — borrow it fresh.
        BracedFast::SelfOrigin => Ok(param_value(name, index.as_ref(), ctx)),
        BracedFast::Empty => Ok(Cow::Borrowed("")),
        // Needs word evaluation / context mutation: `origin` must be owned.
        BracedFast::Computed => {
            let origin = param_value(name, index.as_ref(), ctx).into_owned();
            eval_braced_with_origin(name, &origin, op, ctx, runner, warnings, warn_skipped_command)
                .map(Cow::Owned)
        }
    }
}

/// Apply a braced operation that needs word evaluation and/or context
/// mutation. Ops whose result is the origin itself (or empty) are handled by
/// [`braced_fast`] at the call sites and never reach here.
fn eval_braced_with_origin(
    name: &str,
    origin: &str,
    op: &BracedOp,
    ctx: &mut Context,
    runner: &mut Runner,
    warnings: &mut Vec<Diagnostic>,
    warn_skipped_command: bool,
) -> Result<String, ParseErrorInfo> {
    match op {
        BracedOp::Value => unreachable!("Value is short-circuited by braced_fast"),
        BracedOp::Default { word, .. } => {
            eval_scalar_word(word, ctx, runner, warnings, warn_skipped_command)
        }
        BracedOp::Alternative { word, .. } => {
            eval_scalar_word(word, ctx, runner, warnings, warn_skipped_command)
        }
        BracedOp::Error { word, .. } => {
            let msg = eval_scalar_word(word, ctx, runner, warnings, warn_skipped_command)?;
            Err(ParseErrorInfo::SubstitutionError(
                format!("{} undefined: {}", name, msg),
                name.to_string(),
            ))
        }
        BracedOp::Assign { word, .. } => {
            let val = eval_scalar_word(word, ctx, runner, warnings, warn_skipped_command)?;
            ctx.insert(name.to_string(), Value::Scalar(val.clone()));
            Ok(val)
        }
        BracedOp::Substring(word) => {
            let cmd = eval_scalar_word(word, ctx, runner, warnings, warn_skipped_command)?;
            substitution::get_substring(origin, &cmd)
        }
        BracedOp::Trim { kind, word } => {
            let pat = eval_scalar_word(word, ctx, runner, warnings, warn_skipped_command)?;
            let (mode, greedy) = match kind {
                TrimKind::PrefixShort => (true, false),
                TrimKind::PrefixLong => (true, true),
                TrimKind::SuffixShort => (false, false),
                TrimKind::SuffixLong => (false, true),
            };
            substitution::get_trim_prefix(origin, &pat, mode, greedy)
        }
        BracedOp::Replace {
            all,
            anchor,
            pattern,
            replacement,
        } => {
            let pat = eval_scalar_word(pattern, ctx, runner, warnings, warn_skipped_command)?;
            let mut rep =
                eval_scalar_word(replacement, ctx, runner, warnings, warn_skipped_command)?;
            // Bash performs tilde expansion on an *unquoted* replacement that
            // begins with `~` (a `\~` escape or quoted `~` is not expanded).
            if should_tilde_expand(replacement) {
                rep = tilde_expand(&rep);
            }
            substitution::get_replace(origin, &pat, &rep, *all, *anchor)
        }
        BracedOp::CaseMod { kind, word } => {
            let pat = if word.fields.is_empty() {
                None
            } else {
                Some(eval_scalar_word(
                    word,
                    ctx,
                    runner,
                    warnings,
                    warn_skipped_command,
                )?)
            };
            match kind {
                CaseKind::UpperOnce => substitution::get_upper_case(origin, pat.as_deref(), false),
                CaseKind::UpperAll => substitution::get_upper_case(origin, pat.as_deref(), true),
                CaseKind::LowerOnce => substitution::get_lower_case(origin, pat.as_deref(), false),
                CaseKind::LowerAll => substitution::get_lower_case(origin, pat.as_deref(), true),
            }
        }
    }
}

/// Whether the replacement word of `${var/pat/repl}` begins with an
/// unquoted, unescaped `~` (the only form Bash tilde-expands).
fn should_tilde_expand(word: &Word) -> bool {
    matches!(word.fields.first(), Some(Field::Literal(s)) if s.starts_with('~'))
}

/// Tilde expansion for the replacement of `${var/pat/repl}` (Bash scans the
/// replacement for tilde expansion). Only the `~` and `~/...` forms are
/// handled, using `$HOME`.
fn tilde_expand(s: &str) -> String {
    if !s.starts_with('~') {
        return s.to_string();
    }
    let home = std::env::var("HOME").unwrap_or_default();
    if home.is_empty() {
        return s.to_string();
    }
    let rest = &s[1..];
    if rest.is_empty() || rest.starts_with('/') {
        format!("{home}{rest}")
    } else {
        // `~user` forms are left untouched.
        s.to_string()
    }
}

/// Minimal integer arithmetic evaluator for `$(( ... ))`.
fn eval_arith(s: &str, ctx: &Context) -> Result<String, ParseErrorInfo> {
    let mut p = Arith {
        chars: s.chars().collect(),
        pos: 0,
        ctx,
    };
    let val = p.parse_expr()?;
    p.skip_ws();
    if p.pos != p.chars.len() {
        return Err(ParseErrorInfo::SubstitutionError(
            "Invalid arithmetic expression.".to_string(),
            "((".to_string(),
        ));
    }
    Ok(val.to_string())
}

struct Arith<'a> {
    chars: Vec<char>,
    pos: usize,
    ctx: &'a Context,
}

impl<'a> Arith<'a> {
    fn skip_ws(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn parse_expr(&mut self) -> Result<i64, ParseErrorInfo> {
        self.parse_additive()
    }

    fn parse_additive(&mut self) -> Result<i64, ParseErrorInfo> {
        let mut v = self.parse_multiplicative()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('+') => {
                    self.bump();
                    v += self.parse_multiplicative()?;
                }
                Some('-') => {
                    self.bump();
                    v -= self.parse_multiplicative()?;
                }
                _ => return Ok(v),
            }
        }
    }

    fn parse_multiplicative(&mut self) -> Result<i64, ParseErrorInfo> {
        let mut v = self.parse_unary()?;
        loop {
            self.skip_ws();
            match self.peek() {
                Some('*') => {
                    self.bump();
                    v *= self.parse_unary()?;
                }
                Some('/') => {
                    self.bump();
                    v /= self.parse_unary()?;
                }
                Some('%') => {
                    self.bump();
                    v %= self.parse_unary()?;
                }
                _ => return Ok(v),
            }
        }
    }

    fn parse_unary(&mut self) -> Result<i64, ParseErrorInfo> {
        self.skip_ws();
        match self.peek() {
            Some('-') => {
                self.bump();
                Ok(-self.parse_unary()?)
            }
            Some('+') => {
                self.bump();
                self.parse_unary()
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<i64, ParseErrorInfo> {
        self.skip_ws();
        match self.peek() {
            Some('(') => {
                self.bump();
                let v = self.parse_expr()?;
                self.skip_ws();
                if self.peek() != Some(')') {
                    return Err(ParseErrorInfo::SubstitutionError(
                        "Unbalanced parentheses in arithmetic.".to_string(),
                        "((".to_string(),
                    ));
                }
                self.bump();
                Ok(v)
            }
            Some(c) if c.is_ascii_digit() => {
                let mut s = String::new();
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        s.push(c);
                        self.bump();
                    } else {
                        break;
                    }
                }
                s.parse::<i64>().map_err(|_| {
                    ParseErrorInfo::SubstitutionError(
                        "Bad number in arithmetic.".to_string(),
                        "((".to_string(),
                    )
                })
            }
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                let mut name = String::new();
                while let Some(c) = self.peek() {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        name.push(c);
                        self.bump();
                    } else {
                        break;
                    }
                }
                // Variable reference: undefined → 0.
                Ok(param_value(&name, None, self.ctx).parse().unwrap_or(0))
            }
            _ => Err(ParseErrorInfo::SubstitutionError(
                "Invalid arithmetic expression.".to_string(),
                "((".to_string(),
            )),
        }
    }
}
