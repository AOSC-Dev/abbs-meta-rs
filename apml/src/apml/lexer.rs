//! Tokenizer for the apml language.
//!
//! The lexer works directly on the source string. Words are scanned as a
//! sequence of fields (literals, quotes, parameter expansions, command
//! substitutions and arithmetic expansions). Parentheses are treated as
//! literal characters inside words, except inside array literals where a `)`
//! at the top nesting level terminates the current word (and thereby the
//! array element).

use super::ast::*;
use super::error::ParseErrorInfo;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Name(String),
    Equals,
    PlusEquals,
    Word(Word),
    Newline,
    Eof,
}

pub struct Lexer<'a> {
    src: &'a str,
    byte: usize,
    line: usize,
    col: usize,
    token_start: Span,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Lexer {
            src,
            byte: 0,
            line: 1,
            col: 1,
            token_start: Span {
                byte: 0,
                line: 1,
                col: 1,
            },
        }
    }

    pub fn span(&self) -> Span {
        Span {
            byte: self.byte,
            line: self.line,
            col: self.col,
        }
    }

    /// Span of the most recently started token or word.
    pub fn last_span(&self) -> Span {
        self.token_start
    }

    pub(crate) fn peek(&self) -> Option<char> {
        self.src[self.byte..].chars().next()
    }

    pub(crate) fn peek2(&self) -> Option<char> {
        let mut it = self.src[self.byte..].chars();
        it.next();
        it.next()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.byte += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn snapshot(&self) -> (usize, usize, usize) {
        (self.byte, self.line, self.col)
    }

    fn restore(&mut self, s: (usize, usize, usize)) {
        self.byte = s.0;
        self.line = s.1;
        self.col = s.2;
    }

    /// Skip spaces, tabs, carriage returns and `\`-newline continuations.
    fn skip_ws(&mut self) {
        loop {
            match self.peek() {
                Some(' ') | Some('\t') | Some('\r') => {
                    self.bump();
                }
                Some('\\') if self.peek2() == Some('\n') => {
                    self.bump();
                    self.bump();
                }
                _ => break,
            }
        }
    }

    /// Skip a comment up to (but not including) the newline. Assumes `#`.
    fn skip_comment(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                break;
            }
            self.bump();
        }
    }

    /// Skip whitespace including newlines (used inside array literals).
    fn skip_array_ws(&mut self) {
        loop {
            match self.peek() {
                Some(' ') | Some('\t') | Some('\r') | Some('\n') => {
                    self.bump();
                }
                Some('\\') if self.peek2() == Some('\n') => {
                    self.bump();
                    self.bump();
                }
                _ => break,
            }
        }
    }

    /// Skip everything until the next newline or EOF (error recovery).
    pub fn recover_to_newline(&mut self) {
        while let Some(c) = self.peek() {
            if c == '\n' {
                self.bump();
                return;
            }
            self.bump();
        }
    }

    /// Produce the next statement-level token.
    pub fn next_token(&mut self) -> Result<Token, ParseErrorInfo> {
        loop {
            self.skip_ws();
            match self.peek() {
                None => return Ok(Token::Eof),
                Some('\n') => {
                    self.bump();
                    return Ok(Token::Newline);
                }
                Some(';') => {
                    // Statement separator (Bash metacharacter).
                    self.bump();
                    return Ok(Token::Newline);
                }
                Some('#') => {
                    self.skip_comment();
                    continue;
                }
                _ => {}
            }
            self.token_start = self.span();

            // Detect an assignment head `NAME=` or `NAME+=`. The operator is
            // left in place for the next `next_token` call to consume.
            let save = self.snapshot();
            if let Some(name) = self.try_scan_name() {
                match self.peek() {
                    Some('=') => return Ok(Token::Name(name)),
                    Some('+') if self.peek2() == Some('=') => return Ok(Token::Name(name)),
                    _ => self.restore(save),
                }
            }

            // Standalone `=` / `+=` (e.g. a stray `=` at statement start).
            match self.peek() {
                Some('=') => {
                    self.bump();
                    return Ok(Token::Equals);
                }
                Some('+') if self.peek2() == Some('=') => {
                    self.bump();
                    self.bump();
                    return Ok(Token::PlusEquals);
                }
                _ => {}
            }

            let word = self.scan_word(false)?;
            return Ok(Token::Word(word));
        }
    }

    /// Try to scan an identifier at the current position. Advances the
    /// position on success; leaves it untouched on failure.
    fn try_scan_name(&mut self) -> Option<String> {
        let c = self.peek()?;
        if !(c.is_ascii_alphabetic() || c == '_') {
            return None;
        }
        let mut s = String::new();
        s.push(self.bump()?);
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                s.push(self.bump()?);
            } else {
                break;
            }
        }
        Some(s)
    }

    /// Scan a full word starting at the current position.
    ///
    /// `paren_mode`: inside an array literal. When enabled, a `)` at the top
    /// nesting level terminates the word (leaving the `)` unconsumed for the
    /// array scanner), and `(`/`)` inside the word are tracked for nesting.
    pub fn scan_word(&mut self, paren_mode: bool) -> Result<Word, ParseErrorInfo> {
        let start = self.span();
        self.token_start = start;
        let mut fields: Vec<Field> = Vec::new();
        let mut paren_depth: usize = 0;
        loop {
            let c = match self.peek() {
                None => break,
                Some(c) => c,
            };
            match c {
                ' ' | '\t' | '\r' | '\n' => break,
                ';' => break,
                ')' if paren_mode && paren_depth == 0 => break,
                '\'' => {
                    let s = self.scan_single_quoted()?;
                    fields.push(Field::SingleQuoted(s));
                }
                '"' => {
                    let f = self.scan_double_quoted()?;
                    fields.push(Field::DoubleQuoted(f));
                }
                '\\' => {
                    self.bump();
                    match self.peek() {
                        Some('\n') => {
                            self.bump();
                        }
                        Some(_) => {
                            let c2 = self.bump().unwrap();
                            fields.push(Field::Escaped(c2));
                        }
                        None => fields.push(Field::Escaped('\\')),
                    }
                }
                '$' => {
                    let f = self.scan_dollar()?;
                    fields.push(f);
                }
                '(' if paren_mode => {
                    paren_depth += 1;
                    self.bump();
                }
                ')' if paren_mode => {
                    paren_depth -= 1;
                    self.bump();
                }
                _ => {
                    let s = self.scan_literal_run(paren_mode);
                    push_literal(&mut fields, s);
                }
            }
        }
        Ok(Word {
            span: start,
            fields,
        })
    }

    /// Scan an array literal `( ... )`. Assumes positioned at `(`.
    pub fn scan_array(&mut self) -> Result<Vec<Word>, ParseErrorInfo> {
        let start = self.span();
        self.token_start = start;
        self.bump(); // (
        let mut elems = Vec::new();
        loop {
            self.skip_array_ws();
            match self.peek() {
                None => {
                    return Err(ParseErrorInfo::InvalidSyntax(
                        "Unterminated array literal.".to_string(),
                    ))
                }
                Some(')') => {
                    self.bump();
                    return Ok(elems);
                }
                Some('#') => self.skip_comment(),
                Some(';') => {
                    self.bump();
                }
                _ => {
                    let w = self.scan_word(true)?;
                    elems.push(w);
                }
            }
        }
    }

    fn scan_single_quoted(&mut self) -> Result<String, ParseErrorInfo> {
        self.bump(); // '
        let mut s = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(ParseErrorInfo::InvalidSyntax(
                        "Unterminated single quote.".to_string(),
                    ))
                }
                Some('\'') => {
                    self.bump();
                    return Ok(s);
                }
                Some(_) => {
                    s.push(self.bump().unwrap());
                }
            }
        }
    }

    fn scan_double_quoted(&mut self) -> Result<Vec<Field>, ParseErrorInfo> {
        self.bump(); // "
        let mut fields = Vec::new();
        loop {
            let c = match self.peek() {
                None => {
                    return Err(ParseErrorInfo::InvalidSyntax(
                        "Unterminated double quote.".to_string(),
                    ))
                }
                Some(c) => c,
            };
            match c {
                '"' => {
                    self.bump();
                    return Ok(fields);
                }
                '\\' => {
                    self.bump();
                    match self.peek() {
                        Some('\n') => {
                            self.bump();
                        }
                        Some('"') | Some('\\') | Some('$') | Some('`') => {
                            push_literal_char(&mut fields, self.bump().unwrap());
                        }
                        Some(_) => {
                            push_literal_char(&mut fields, '\\');
                            push_literal_char(&mut fields, self.bump().unwrap());
                        }
                        None => push_literal_char(&mut fields, '\\'),
                    }
                }
                '$' => {
                    let f = self.scan_dollar()?;
                    fields.push(f);
                }
                _ => {
                    let s = self.scan_dq_literal_run();
                    push_literal(&mut fields, s);
                }
            }
        }
    }

    fn scan_literal_run(&mut self, paren_mode: bool) -> &'a str {
        let start = self.byte;
        loop {
            let c = match self.peek() {
                None => break,
                Some(c) => c,
            };
            match c {
                ' ' | '\t' | '\r' | '\n' | '\'' | '"' | '\\' | '$' | ';' => break,
                '(' | ')' if paren_mode => break,
                _ => {
                    self.bump();
                }
            }
        }
        &self.src[start..self.byte]
    }

    fn scan_dq_literal_run(&mut self) -> &'a str {
        let start = self.byte;
        loop {
            match self.peek() {
                Some('"') | Some('\\') | Some('$') | None => break,
                Some(_) => {
                    self.bump();
                }
            }
        }
        &self.src[start..self.byte]
    }

    /// Scan a `$`-introduced expansion. Assumes positioned at `$`.
    fn scan_dollar(&mut self) -> Result<Field, ParseErrorInfo> {
        self.bump(); // $
        match self.peek() {
            Some('(') => {
                self.bump(); // (
                if self.peek() == Some('(') {
                    self.bump(); // (
                    let content = self.scan_subst_raw(true)?;
                    Ok(Field::Arith(content.to_string()))
                } else {
                    let content = self.scan_subst_raw(false)?;
                    let cmds = parse_command_content(content)?;
                    Ok(Field::Command(cmds))
                }
            }
            Some('{') => {
                let p = self.scan_braced()?;
                Ok(Field::Param(p))
            }
            Some('@') => {
                self.bump();
                Ok(Field::Param(Param::Special('@')))
            }
            Some('*') => {
                self.bump();
                Ok(Field::Param(Param::Special('*')))
            }
            Some('#') => {
                self.bump();
                Ok(Field::Param(Param::Special('#')))
            }
            Some('?') => {
                self.bump();
                Ok(Field::Param(Param::Special('?')))
            }
            Some('-') => {
                self.bump();
                Ok(Field::Param(Param::Special('-')))
            }
            Some('$') => {
                self.bump();
                Ok(Field::Param(Param::Special('$')))
            }
            Some('!') => {
                self.bump();
                Ok(Field::Param(Param::Special('!')))
            }
            Some(c) if c.is_ascii_digit() => {
                let start = self.byte;
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() {
                        self.bump();
                    } else {
                        break;
                    }
                }
                let n: u32 = self.src[start..self.byte].parse().unwrap_or(0);
                Ok(Field::Param(Param::Positional(n)))
            }
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                let name_start = self.byte;
                let name = self.scan_name_chars();
                Ok(Field::Param(Param::Plain { name, name_start }))
            }
            _ => Ok(Field::Literal("$".to_string())),
        }
    }

    fn scan_name_chars(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                s.push(self.bump().unwrap());
            } else {
                break;
            }
        }
        s
    }

    /// Scan a `${ ... }` braced parameter expansion. Assumes positioned at
    /// `{` (the `$` has been consumed).
    fn scan_braced(&mut self) -> Result<Param, ParseErrorInfo> {
        self.bump(); // {
        let start = self.byte;
        self.skip_to_matching_brace()?;
        // skip_to_matching_brace consumed the closing `}` (1 byte).
        let content = &self.src[start..self.byte - 1];
        parse_braced_content(content, start)
    }

    /// Scan the raw content of a `$( ... )` or `$(( ... ))` substitution.
    ///
    /// Assumes positioned at the first content character (the opening
    /// parens have been consumed). `is_arith` selects the `))` terminator.
    fn scan_subst_raw(&mut self, is_arith: bool) -> Result<&'a str, ParseErrorInfo> {
        let start = self.byte;
        let mut depth: usize = 1;
        loop {
            let c = match self.peek() {
                None => {
                    return Err(ParseErrorInfo::InvalidSyntax(
                        "Unterminated substitution.".to_string(),
                    ))
                }
                Some(c) => c,
            };
            match c {
                '\\' => {
                    self.bump();
                    self.bump();
                }
                '\'' => self.skip_single_quoted()?,
                '"' => self.skip_double_quoted()?,
                '$' => {
                    if self.peek2() == Some('{') {
                        self.bump();
                        self.bump();
                        self.skip_to_matching_brace()?;
                    } else if self.peek2() == Some('(') {
                        self.bump();
                        self.bump();
                        let arith = self.peek() == Some('(');
                        if arith {
                            self.bump();
                        }
                        let _ = self.scan_subst_raw(arith)?;
                    } else {
                        self.bump();
                    }
                }
                '(' => {
                    depth += 1;
                    self.bump();
                }
                ')' => {
                    if is_arith && depth == 1 && self.peek2() == Some(')') {
                        let content = &self.src[start..self.byte];
                        self.bump();
                        self.bump();
                        return Ok(content);
                    }
                    if depth == 1 {
                        let content = &self.src[start..self.byte];
                        self.bump();
                        return Ok(content);
                    }
                    depth -= 1;
                    self.bump();
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// Skip a `'...'` region. Assumes positioned at `'`.
    fn skip_single_quoted(&mut self) -> Result<(), ParseErrorInfo> {
        self.bump(); // '
        loop {
            match self.peek() {
                None => {
                    return Err(ParseErrorInfo::InvalidSyntax(
                        "Unterminated single quote.".to_string(),
                    ))
                }
                Some('\'') => {
                    self.bump();
                    return Ok(());
                }
                Some(_) => {
                    self.bump();
                }
            }
        }
    }

    /// Skip a `"..."` region, handling nested `${ }` and `$( )`.
    /// Assumes positioned at `"`.
    fn skip_double_quoted(&mut self) -> Result<(), ParseErrorInfo> {
        self.bump(); // "
        loop {
            let c = match self.peek() {
                None => {
                    return Err(ParseErrorInfo::InvalidSyntax(
                        "Unterminated double quote.".to_string(),
                    ))
                }
                Some(c) => c,
            };
            match c {
                '"' => {
                    self.bump();
                    return Ok(());
                }
                '\\' => {
                    self.bump();
                    self.bump();
                }
                '$' => {
                    if self.peek2() == Some('{') {
                        self.bump();
                        self.bump();
                        self.skip_to_matching_brace()?;
                    } else if self.peek2() == Some('(') {
                        self.bump();
                        self.bump();
                        let arith = self.peek() == Some('(');
                        if arith {
                            self.bump();
                        }
                        let _ = self.scan_subst_raw(arith)?;
                    } else {
                        self.bump();
                    }
                }
                _ => {
                    self.bump();
                }
            }
        }
    }

    /// Skip from `{` through the matching `}` of a `${ ... }`, respecting
    /// nested `${ }`/`$( )` and quoted regions. Assumes positioned at `{`.
    fn skip_to_matching_brace(&mut self) -> Result<(), ParseErrorInfo> {
        self.bump(); // {
        let mut depth: usize = 0;
        loop {
            let c = match self.peek() {
                None => {
                    return Err(ParseErrorInfo::InvalidSyntax(
                        "Unterminated `${`.".to_string(),
                    ))
                }
                Some(c) => c,
            };
            match c {
                '\\' => {
                    self.bump();
                    self.bump();
                }
                '\'' => self.skip_single_quoted()?,
                '"' => self.skip_double_quoted()?,
                '$' => {
                    if self.peek2() == Some('{') {
                        depth += 1;
                        self.bump();
                        self.bump();
                    } else if self.peek2() == Some('(') {
                        self.bump();
                        self.bump();
                        let arith = self.peek() == Some('(');
                        if arith {
                            self.bump();
                        }
                        let _ = self.scan_subst_raw(arith)?;
                    } else {
                        self.bump();
                    }
                }
                '}' => {
                    if depth == 0 {
                        self.bump();
                        return Ok(());
                    }
                    depth -= 1;
                    self.bump();
                }
                _ => {
                    self.bump();
                }
            }
        }
    }
}

fn push_literal(fields: &mut Vec<Field>, s: &str) {
    if s.is_empty() {
        return;
    }
    if let Some(Field::Literal(last)) = fields.last_mut() {
        last.push_str(s);
    } else {
        fields.push(Field::Literal(s.to_string()));
    }
}

/// Append a single char to the last literal field (or start a new one),
/// without allocating an intermediate `String`.
fn push_literal_char(fields: &mut Vec<Field>, c: char) {
    if let Some(Field::Literal(last)) = fields.last_mut() {
        last.push(c);
    } else {
        fields.push(Field::Literal(c.to_string()));
    }
}

/// Parse the inner content of a `${ ... }` expansion into a [`Param`].
///
/// `content_start` is the byte offset of `content` in the source (just after
/// `${`), used to record where the variable name begins.
fn parse_braced_content(content: &str, content_start: usize) -> Result<Param, ParseErrorInfo> {
    // `${#name}` / `${#name[@]}` — length.
    if let Some(rest) = content.strip_prefix('#') {
        if let Some((name, index, rem)) = split_name_index(rest) {
            if rem.is_empty() {
                return Ok(Param::Length {
                    name,
                    name_start: content_start + 1,
                    index,
                });
            }
        }
        return Err(ParseErrorInfo::InvalidSyntax(
            "Unsupported parameter expansion.".to_string(),
        ));
    }

    let (name, index, rest) = match split_name_index(content) {
        Some(x) => x,
        None => {
            return Err(ParseErrorInfo::InvalidSyntax(
                "Bad parameter expansion.".to_string(),
            ))
        }
    };

    let name_start = content_start;
    if rest.is_empty() {
        return Ok(Param::Braced {
            name,
            name_start,
            index,
            op: BracedOp::Value,
        });
    }

    // `${name[=:-?+]#...}` — a substitution applied to a subscripted value.
    let subst = |name: String, index: Option<Index>, op: BracedOp| Param::Braced {
        name,
        name_start,
        index,
        op,
    };

    let op = if let Some(r) = rest.strip_prefix(":-") {
        BracedOp::Default {
            colon: true,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix(":=") {
        BracedOp::Assign {
            colon: true,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix(":?") {
        BracedOp::Error {
            colon: true,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix(":+") {
        BracedOp::Alternative {
            colon: true,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix("##") {
        BracedOp::Trim {
            kind: TrimKind::PrefixLong,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix("%%") {
        BracedOp::Trim {
            kind: TrimKind::SuffixLong,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix("^^") {
        BracedOp::CaseMod {
            kind: CaseKind::UpperAll,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix(",,") {
        BracedOp::CaseMod {
            kind: CaseKind::LowerAll,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix("//") {
        parse_replace(r, true, ReplaceAnchor::Any)?
    } else if let Some(r) = rest.strip_prefix("/#") {
        parse_replace(r, false, ReplaceAnchor::Prefix)?
    } else if let Some(r) = rest.strip_prefix("/%") {
        parse_replace(r, false, ReplaceAnchor::Suffix)?
    } else if let Some(r) = rest.strip_prefix(':') {
        BracedOp::Substring(parse_operand(r)?)
    } else if let Some(r) = rest.strip_prefix('#') {
        BracedOp::Trim {
            kind: TrimKind::PrefixShort,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix('%') {
        BracedOp::Trim {
            kind: TrimKind::SuffixShort,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix('^') {
        BracedOp::CaseMod {
            kind: CaseKind::UpperOnce,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix(',') {
        BracedOp::CaseMod {
            kind: CaseKind::LowerOnce,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix('/') {
        parse_replace(r, false, ReplaceAnchor::Any)?
    } else if let Some(r) = rest.strip_prefix('-') {
        BracedOp::Default {
            colon: false,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix('=') {
        BracedOp::Assign {
            colon: false,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix('?') {
        BracedOp::Error {
            colon: false,
            word: parse_operand(r)?,
        }
    } else if let Some(r) = rest.strip_prefix('+') {
        BracedOp::Alternative {
            colon: false,
            word: parse_operand(r)?,
        }
    } else {
        return Err(ParseErrorInfo::InvalidSyntax(format!(
            "Unsupported parameter expansion: ${{{content}}}"
        )));
    };

    Ok(subst(name, index, op))
}

/// Split `NAME[index]` from the head of `s`.
fn split_name_index(s: &str) -> Option<(String, Option<Index>, &str)> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;

    // name: identifier or a special parameter character.
    let mut name = String::new();
    match chars.first()? {
        c if c.is_ascii_alphabetic() || *c == '_' => {
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                name.push(chars[i]);
                i += 1;
            }
        }
        '@' | '*' | '#' | '?' | '-' | '$' | '!' => {
            name.push(chars[i]);
            i += 1;
        }
        _ => return None,
    }

    // optional [index]
    let mut index = None;
    if i < chars.len() && chars[i] == '[' {
        i += 1;
        let mut idx = String::new();
        while i < chars.len() && chars[i] != ']' {
            idx.push(chars[i]);
            i += 1;
        }
        if i >= chars.len() {
            return None;
        }
        i += 1; // ]
        index = Some(match idx.as_str() {
            "@" => Index::At,
            "*" => Index::Star,
            n => Index::Number(n.parse().map_err(|_| ()).ok()?),
        });
    }

    Some((name, index, &s[i..]))
}

/// Parse a `${name/pat/rep}`-style replacement operand.
fn parse_replace(r: &str, all: bool, anchor: ReplaceAnchor) -> Result<BracedOp, ParseErrorInfo> {
    match find_top_level_sep(r, &['/']) {
        Some(idx) => Ok(BracedOp::Replace {
            all,
            anchor,
            pattern: parse_operand(&r[..idx])?,
            replacement: parse_operand(&r[idx + 1..])?,
        }),
        None => Ok(BracedOp::Replace {
            all,
            anchor,
            pattern: parse_operand(r)?,
            replacement: parse_operand("")?,
        }),
    }
}

/// Scan a word from a substring (used for `${...}` operands).
fn parse_operand(s: &str) -> Result<Word, ParseErrorInfo> {
    let mut lx = Lexer::new(s);
    let w = lx.scan_operand_word()?;
    Ok(w)
}

/// Parse the content of a `$( ... )` into a list of pipelines.
fn parse_command_content(content: &str) -> Result<Vec<Command>, ParseErrorInfo> {
    if find_top_level_sep(content, &['&']).is_some() {
        return Err(ParseErrorInfo::RestrictedSyntax(
            "Backgrounding is not allowed in command substitution.".to_string(),
            "$(".to_string(),
        ));
    }
    let mut commands = Vec::new();
    for pipeline in split_top_level(content, &[';', '\n']) {
        let pipeline = pipeline.trim();
        if pipeline.is_empty() {
            continue;
        }
        let mut stages = Vec::new();
        for stage in split_top_level(pipeline, &['|']) {
            let stage = stage.trim();
            if stage.is_empty() {
                continue;
            }
            let mut lx = Lexer::new(stage);
            let mut words = Vec::new();
            loop {
                match lx.next_token() {
                    Ok(Token::Word(w)) => words.push(w),
                    Ok(Token::Eof) | Ok(Token::Newline) => break,
                    _ => {
                        return Err(ParseErrorInfo::InvalidSyntax(
                            "Only simple commands are allowed in command substitution.".to_string(),
                        ))
                    }
                }
            }
            stages.push(words);
        }
        commands.push(Command { stages });
    }
    Ok(commands)
}

/// Find the first character in `seps` that is not inside quotes, `${ }` or
/// `$( )`.
fn find_top_level_sep(s: &str, seps: &[char]) -> Option<usize> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if seps.contains(&c) {
            return Some(i);
        }
        match c {
            '\\' => i += 2,
            '\'' => {
                i += 1;
                while i < chars.len() && chars[i] != '\'' {
                    i += 1;
                }
                i += 1;
            }
            '"' => {
                i += 1;
                while i < chars.len() {
                    if chars[i] == '\\' {
                        i += 2;
                        continue;
                    }
                    if chars[i] == '"' {
                        i += 1;
                        break;
                    }
                    i += 1;
                }
            }
            '$' => {
                if i + 1 < chars.len() && chars[i + 1] == '{' {
                    i += 2;
                    let mut depth = 0;
                    while i < chars.len() {
                        if chars[i] == '}' && depth == 0 {
                            i += 1;
                            break;
                        }
                        if chars[i] == '{' && i > 0 && chars[i - 1] == '$' {
                            depth += 1;
                        }
                        if chars[i] == '}' && depth > 0 {
                            depth -= 1;
                        }
                        i += 1;
                    }
                } else if i + 1 < chars.len() && chars[i + 1] == '(' {
                    i += 2;
                    let mut depth = 1;
                    while i < chars.len() {
                        if chars[i] == '(' {
                            depth += 1;
                        }
                        if chars[i] == ')' {
                            depth -= 1;
                            if depth == 0 {
                                i += 1;
                                break;
                            }
                        }
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    None
}

/// Split `s` on the given separator characters at the top level (not inside
/// quotes, `${ }` or `$( )`).
fn split_top_level(s: &str, seps: &[char]) -> Vec<String> {
    let mut parts = Vec::new();
    let mut rest = s;
    while let Some(idx) = find_top_level_sep(rest, seps) {
        parts.push(rest[..idx].to_string());
        rest = &rest[idx + 1..];
    }
    parts.push(rest.to_string());
    parts
}

impl<'a> Lexer<'a> {
    /// Scan a word from a `${...}` operand: spaces are literal (the operand
    /// extends to the end of the substring).
    fn scan_operand_word(&mut self) -> Result<Word, ParseErrorInfo> {
        let start = self.span();
        self.token_start = start;
        let mut fields: Vec<Field> = Vec::new();
        loop {
            let c = match self.peek() {
                None => break,
                Some(c) => c,
            };
            match c {
                '\'' => {
                    let s = self.scan_single_quoted()?;
                    fields.push(Field::SingleQuoted(s));
                }
                '"' => {
                    let f = self.scan_double_quoted()?;
                    fields.push(Field::DoubleQuoted(f));
                }
                '\\' => {
                    self.bump();
                    match self.peek() {
                        Some('\n') => {
                            self.bump();
                        }
                        Some(_) => {
                            let c2 = self.bump().unwrap();
                            fields.push(Field::Escaped(c2));
                        }
                        None => fields.push(Field::Escaped('\\')),
                    }
                }
                '$' => {
                    let f = self.scan_dollar()?;
                    fields.push(f);
                }
                _ => {
                    let start = self.byte;
                    while let Some(c) = self.peek() {
                        match c {
                            '\'' | '"' | '\\' | '$' => break,
                            _ => {
                                self.bump();
                            }
                        }
                    }
                    push_literal(&mut fields, &self.src[start..self.byte]);
                }
            }
        }
        Ok(Word {
            span: start,
            fields,
        })
    }
}
