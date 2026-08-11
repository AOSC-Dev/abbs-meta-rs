use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet};
use std::fmt;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub col: usize,
    pub byte: usize,
    pub prev_byte: usize,
    pub error: ParseErrorInfo,
}

#[derive(Debug, Clone)]
pub enum ParseErrorInfo {
    LexerError(String),
    InvalidSyntax(String),
    RestrictedSyntax(String, String),
    ContextError(String, String),
    SubstitutionError(String, String),
    GlobError(String),
    RegexError(String),
}

impl ParseError {
    pub fn pretty_print(&self, source: &str, filename: &str) -> String {
        let (err_type, reason) = match &self.error {
            ParseErrorInfo::InvalidSyntax(r) => ("Invalid syntax", r.as_str()),
            ParseErrorInfo::ContextError(r, _) => ("Context error", r.as_str()),
            ParseErrorInfo::SubstitutionError(r, _) => ("Substitution error", r.as_str()),
            ParseErrorInfo::GlobError(r) => ("Glob translation error", r.as_str()),
            ParseErrorInfo::RegexError(r) => ("Regex error", r.as_str()),
            ParseErrorInfo::LexerError(r) => ("Invalid or unsupported syntax", r.as_str()),
            ParseErrorInfo::RestrictedSyntax(r, _) => ("Restricted syntax", r.as_str()),
        };

        let len = source.len();
        // Clamp the marker range to the source bounds to avoid panicking in
        // the snippet renderer on bad positions.
        let mut start = self.prev_byte.min(len);
        let mut end = self.byte.min(len);
        if start > end {
            std::mem::swap(&mut start, &mut end);
        }
        if start == end && start < len {
            end += 1;
        }

        let report = &[Level::ERROR.primary_title(err_type).element(
            Snippet::source(source)
                .line_start(1)
                .path(filename)
                .fold(true)
                .annotation(AnnotationKind::Primary.span(start..end).label(reason)),
        )];
        Renderer::styled().render(report)
    }
}

impl From<regex::Error> for ParseErrorInfo {
    fn from(err: regex::Error) -> Self {
        match err {
            regex::Error::Syntax(s) => ParseErrorInfo::RegexError(format!("Syntax error: {}", s)),
            regex::Error::CompiledTooBig(_size) => {
                ParseErrorInfo::RegexError("Compiled syntax too big.".to_string())
            }
            _ => ParseErrorInfo::RegexError("Internal regex error.".to_string()),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (err_type, reason) = match &self.error {
            ParseErrorInfo::InvalidSyntax(r) => ("Invalid syntax", r.as_str()),
            ParseErrorInfo::ContextError(r, _) => ("Context error", r.as_str()),
            ParseErrorInfo::SubstitutionError(r, _) => ("Substitution error", r.as_str()),
            ParseErrorInfo::GlobError(r) => ("Glob translation error", r.as_str()),
            ParseErrorInfo::RegexError(r) => ("Regex error", r.as_str()),
            ParseErrorInfo::LexerError(r) => ("Invalid or unsupported syntax", r.as_str()),
            ParseErrorInfo::RestrictedSyntax(r, _) => ("Restricted syntax", r.as_str()),
        };

        write!(
            f,
            "{} at line {}, col {}. Reason: {}",
            err_type, self.line, self.col, reason
        )
    }
}

impl std::error::Error for ParseError {}
