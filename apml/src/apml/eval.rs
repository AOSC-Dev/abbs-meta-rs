//! Evaluation of parsed statements against a [`Context`].
//!
//! Semantics follow Bash for the supported subset: undefined variables
//! expand to an empty string, arrays support `"${arr[@]}"` expansion, and
//! `VAR+="x"` / `VAR+=(...)` append to scalar / array values.

use super::ast::*;
use super::error::ParseErrorInfo;
use super::substitution;
use super::value::Value;
use std::collections::HashMap;

/// The variable context: name -> value.
pub type Context = HashMap<String, Value>;

/// A runner for `$( ... )` command substitutions. Receives the expanded
/// pipeline (each stage is a list of command words) and returns the
/// command's standard output (without the trailing newline), or an error.
pub type Runner<'a> = dyn FnMut(&[Vec<String>]) -> Result<String, String> + 'a;

pub fn eval_stmts(
    stmts: &[Stmt],
    context: &mut Context,
    runner: &mut Runner,
) -> Result<(), ParseErrorInfo> {
    for stmt in stmts {
        eval_stmt(stmt, context, runner)?;
    }
    Ok(())
}

fn eval_stmt(stmt: &Stmt, ctx: &mut Context, runner: &mut Runner) -> Result<(), ParseErrorInfo> {
    let name = &stmt.name;
    match &stmt.value {
        ValueExpr::Scalar(w) => {
            let s = eval_scalar_word(w, ctx, runner)?;
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
                elems.extend(eval_array_word(w, ctx, runner)?);
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
) -> Result<String, ParseErrorInfo> {
    eval_fields_scalar(&w.fields, ctx, runner)
}

fn eval_fields_scalar(
    fields: &[Field],
    ctx: &mut Context,
    runner: &mut Runner,
) -> Result<String, ParseErrorInfo> {
    let mut out = String::new();
    for f in fields {
        match f {
            Field::Literal(s) => out.push_str(s),
            Field::SingleQuoted(s) => out.push_str(s),
            Field::DoubleQuoted(fs) => out.push_str(&eval_fields_scalar(fs, ctx, runner)?),
            Field::Param(p) => out.push_str(&eval_param_scalar(p, ctx, runner)?),
            Field::Command(cmds) => {
                let mut output = String::new();
                for cmd in cmds {
                    let mut stages = Vec::new();
                    for stage_words in &cmd.stages {
                        let mut words = Vec::new();
                        for w in stage_words {
                            words.push(eval_scalar_word(w, ctx, runner)?);
                        }
                        stages.push(words);
                    }
                    let s = runner(&stages).map_err(|e| {
                        ParseErrorInfo::SubstitutionError(e, "$(".to_string())
                    })?;
                    output.push_str(&s);
                }
                out.push_str(&output);
            }
            Field::Arith(s) => out.push_str(&eval_arith(s, ctx)?),
        }
    }
    Ok(out)
}

/// Evaluate a word in an array-element context: may expand to multiple
/// values when it contains `"${arr[@]}"`/`${arr[@]}` (or `$@`).
fn eval_array_word(
    w: &Word,
    ctx: &mut Context,
    runner: &mut Runner,
) -> Result<Vec<String>, ParseErrorInfo> {
    // `"${arr[@]}"` alone → one element per array element.
    if w.fields.len() == 1 {
        if let Field::DoubleQuoted(fs) = &w.fields[0] {
            if fs.len() == 1 {
                if let Field::Param(Param::Braced {
                    name,
                    index: Some(Index::At),
                    op: BracedOp::Value,
                }) = &fs[0]
                {
                    return Ok(array_elems(name, ctx));
                }
                if let Field::Param(Param::Special('@')) = &fs[0] {
                    return Ok(Vec::new());
                }
            }
        }
        // unquoted `${arr[@]}` → each element, then IFS-split each.
        if let Field::Param(Param::Braced {
            name,
            index: Some(Index::At),
            op: BracedOp::Value,
        }) = &w.fields[0]
        {
            let mut out = Vec::new();
            for e in array_elems(name, ctx) {
                out.extend(ifs_split(&e));
            }
            return Ok(out);
        }
        if let Field::Param(Param::Special('@')) = &w.fields[0] {
            return Ok(Vec::new());
        }
    }

    let s = eval_scalar_word(w, ctx, runner)?;
    if is_fully_quoted(w) {
        Ok(vec![s])
    } else {
        Ok(ifs_split(&s))
    }
}

fn is_fully_quoted(w: &Word) -> bool {
    if w.fields.len() != 1 {
        return false;
    }
    matches!(
        w.fields[0],
        Field::SingleQuoted(_) | Field::DoubleQuoted(_)
    )
}

fn array_elems(name: &str, ctx: &Context) -> Vec<String> {
    match ctx.get(name) {
        Some(Value::Array(a)) => a.clone(),
        _ => Vec::new(),
    }
}

/// Default IFS word splitting (space / tab / newline).
fn ifs_split(s: &str) -> Vec<String> {
    s.split(|c: char| c == ' ' || c == '\t' || c == '\n')
        .filter(|x| !x.is_empty())
        .map(|x| x.to_string())
        .collect()
}

fn eval_param_scalar(
    p: &Param,
    ctx: &mut Context,
    runner: &mut Runner,
) -> Result<String, ParseErrorInfo> {
    match p {
        Param::Plain(name) => Ok(param_value(name, None, ctx)),
        Param::Special(_) | Param::Positional(_) => Ok(String::new()),
        Param::Length { name, index } => match index {
            Some(Index::At) | Some(Index::Star) => Ok(format!("{}", array_elems(name, ctx).len())),
            _ => Ok(format!("{}", param_value(name, None, ctx).chars().count())),
        },
        Param::Braced { name, index, op } => eval_braced(name, index, op, ctx, runner),
    }
}

/// The value of a parameter; undefined variables expand to an empty string
/// (Bash semantics). Referencing an array without a subscript yields its
/// first element.
fn param_value(name: &str, index: Option<&Index>, ctx: &Context) -> String {
    match ctx.get(name) {
        None => String::new(),
        Some(Value::Scalar(s)) => s.clone(),
        Some(Value::Array(a)) => match index {
            None | Some(Index::Number(0)) => a.first().cloned().unwrap_or_default(),
            Some(Index::Number(i)) => a.get(*i).cloned().unwrap_or_default(),
            Some(Index::At) | Some(Index::Star) => a.join(" "),
        },
    }
}

fn eval_braced(
    name: &str,
    index: &Option<Index>,
    op: &BracedOp,
    ctx: &mut Context,
    runner: &mut Runner,
) -> Result<String, ParseErrorInfo> {
    match op {
        BracedOp::Value => Ok(param_value(name, index.as_ref(), ctx)),
        BracedOp::Default { colon, word } => {
            let v = param_value(name, None, ctx);
            let set = ctx.contains_key(name);
            let use_self = if *colon { set && !v.is_empty() } else { set };
            if use_self {
                Ok(v)
            } else {
                eval_scalar_word(word, ctx, runner)
            }
        }
        BracedOp::Alternative { colon, word } => {
            let v = param_value(name, None, ctx);
            let set = ctx.contains_key(name);
            let use_alt = if *colon { set && !v.is_empty() } else { set };
            if use_alt {
                eval_scalar_word(word, ctx, runner)
            } else {
                Ok(String::new())
            }
        }
        BracedOp::Error { colon, word } => {
            let v = param_value(name, None, ctx);
            let set = ctx.contains_key(name);
            let is_err = if *colon { !set || v.is_empty() } else { !set };
            if is_err {
                let msg = eval_scalar_word(word, ctx, runner)?;
                Err(ParseErrorInfo::SubstitutionError(
                    format!("{} undefined: {}", name, msg),
                    name.to_string(),
                ))
            } else {
                Ok(v)
            }
        }
        BracedOp::Assign { colon, word } => {
            let v = param_value(name, None, ctx);
            let set = ctx.contains_key(name);
            let need_assign = if *colon { !set || v.is_empty() } else { !set };
            if need_assign {
                let val = eval_scalar_word(word, ctx, runner)?;
                ctx.insert(name.to_string(), Value::Scalar(val.clone()));
                Ok(val)
            } else {
                Ok(v)
            }
        }
        BracedOp::Substring(word) => {
            let v = param_value(name, None, ctx);
            let cmd = eval_scalar_word(word, ctx, runner)?;
            substitution::get_substring(&v, &cmd)
        }
        BracedOp::Trim { kind, word } => {
            let v = param_value(name, None, ctx);
            let pat = eval_scalar_word(word, ctx, runner)?;
            let (mode, greedy) = match kind {
                TrimKind::PrefixShort => (true, false),
                TrimKind::PrefixLong => (true, true),
                TrimKind::SuffixShort => (false, false),
                TrimKind::SuffixLong => (false, true),
            };
            substitution::get_trim_prefix(&v, &pat, mode, greedy)
        }
        BracedOp::Replace {
            all,
            anchor,
            pattern,
            replacement,
        } => {
            let v = param_value(name, None, ctx);
            let pat = eval_scalar_word(pattern, ctx, runner)?;
            let rep = eval_scalar_word(replacement, ctx, runner)?;
            substitution::get_replace(&v, &pat, &rep, *all, *anchor)
        }
        BracedOp::CaseMod { kind, word } => {
            let v = param_value(name, None, ctx);
            let pat = if word.fields.is_empty() {
                None
            } else {
                Some(eval_scalar_word(word, ctx, runner)?)
            };
            match kind {
                CaseKind::UpperOnce => substitution::get_upper_case(&v, pat.as_deref(), false),
                CaseKind::UpperAll => substitution::get_upper_case(&v, pat.as_deref(), true),
                CaseKind::LowerOnce => substitution::get_lower_case(&v, pat.as_deref(), false),
                CaseKind::LowerAll => substitution::get_lower_case(&v, pat.as_deref(), true),
            }
        }
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
