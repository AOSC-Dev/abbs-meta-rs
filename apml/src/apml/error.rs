use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet};
use std::fmt;

use super::ast::Span;

/// The position of a diagnostic in the source, plus the byte range to
/// highlight (clamped to the source when rendering).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticSpan {
    /// 1-based line.
    pub line: usize,
    /// 1-based column.
    pub col: usize,
    /// Byte offset of the end of the highlighted range.
    pub byte: usize,
    /// Byte offset of the start of the highlighted range.
    pub prev_byte: usize,
}

impl From<Span> for DiagnosticSpan {
    fn from(span: Span) -> Self {
        DiagnosticSpan {
            line: span.line,
            col: span.col,
            byte: span.byte,
            prev_byte: span.byte,
        }
    }
}

impl DiagnosticSpan {
    /// The source byte range to highlight, clamped to the source bounds so
    /// the snippet renderer never panics on out-of-range positions.
    pub fn highlight_range(&self, source: &str) -> std::ops::Range<usize> {
        let len = source.len();
        let mut start = self.prev_byte.min(len);
        let mut end = self.byte.min(len);
        if start > end {
            std::mem::swap(&mut start, &mut end);
        }
        if start == end && start < len {
            end += 1;
        }
        start..end
    }
}

/// The payload of a [`Diagnostic`]: a fatal error or a non-fatal warning.
#[derive(Debug, Clone)]
pub enum DiagnosticInfo {
    Error(ParseErrorInfo),
    Warning(String),
}

/// A single diagnostic produced while parsing: a source location plus
/// either an error or a warning.
///
/// Errors and warnings share this shape and only differ in
/// [`DiagnosticInfo`] — e.g. a `$( ... )` command substitution that expanded
/// to an empty string because it is never executed is reported as a warning.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub span: DiagnosticSpan,
    pub info: DiagnosticInfo,
}

impl Diagnostic {
    pub fn pretty_print(&self, source: &str, filename: &str) -> String {
        let (level, title, label) = match &self.info {
            DiagnosticInfo::Error(err) => {
                let (title, reason) = err.summary();
                (Level::ERROR, title, reason)
            }
            DiagnosticInfo::Warning(msg) => (Level::WARNING, "Warning", msg.as_str()),
        };
        let range = self.span.highlight_range(source);
        let report = &[level.primary_title(title).element(
            Snippet::source(source)
                .line_start(1)
                .path(filename)
                .fold(true)
                .annotation(AnnotationKind::Primary.span(range).label(label)),
        )];
        Renderer::styled().render(report)
    }
}

/// A structured parse error.
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

impl ParseErrorInfo {
    /// A short human-readable summary (title and reason) for diagnostics.
    fn summary(&self) -> (&'static str, &str) {
        match self {
            ParseErrorInfo::InvalidSyntax(r) => ("Invalid syntax", r.as_str()),
            ParseErrorInfo::ContextError(r, _) => ("Context error", r.as_str()),
            ParseErrorInfo::SubstitutionError(r, _) => ("Substitution error", r.as_str()),
            ParseErrorInfo::GlobError(r) => ("Glob translation error", r.as_str()),
            ParseErrorInfo::RegexError(r) => ("Regex error", r.as_str()),
            ParseErrorInfo::LexerError(r) => ("Invalid or unsupported syntax", r.as_str()),
            ParseErrorInfo::RestrictedSyntax(r, _) => ("Restricted syntax", r.as_str()),
        }
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

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.info {
            DiagnosticInfo::Error(err) => {
                let (err_type, reason) = err.summary();
                write!(
                    f,
                    "{} at line {}, col {}. Reason: {}",
                    err_type, self.span.line, self.span.col, reason
                )
            }
            DiagnosticInfo::Warning(msg) => {
                write!(f, "Warning at line {}, col {}: {}", self.span.line, self.span.col, msg)
            }
        }
    }
}

impl std::error::Error for Diagnostic {}
