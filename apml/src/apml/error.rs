use aho_corasick::{AhoCorasickBuilder, MatchKind};
use annotate_snippets::{
    display_list::{DisplayList, FormatOptions},
    snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation},
};
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

#[inline]
fn locate_keyword(
    source: &str,
    keyword: &str,
    start: usize,
    end: usize,
    bare: bool,
) -> Option<(usize, usize)> {
    let mut search = AhoCorasickBuilder::new();
    let searcher;
    if !bare {
        searcher = search.match_kind(MatchKind::LeftmostLongest).build(&[
            format!("${{{}", keyword).as_str(),
            format!("${}", keyword).as_str(),
            "$(",
        ])
    } else {
        searcher = search.build(&[keyword]);
    }

    if start > end {
        return None;
    }
    let range = searcher.find(&source.as_bytes()[start..end]);
    if let Some(range) = range {
        return Some((range.start(), range.end()));
    }

    None
}

impl ParseError {
    pub fn pretty_print(&self, source: &str, filename: &str) -> String {
        let mut bare_search = false;
        let (err_type, reason, keyword) = match &self.error {
            ParseErrorInfo::InvalidSyntax(r) => ("Invalid syntax", r, None),
            ParseErrorInfo::ContextError(r, kw) => ("Context error", r, Some(kw)),
            ParseErrorInfo::SubstitutionError(r, kw) => ("Substitution error", r, Some(kw)),
            ParseErrorInfo::GlobError(r) => ("Glob translation error", r, None),
            ParseErrorInfo::RegexError(r) => ("Regex error", r, None),
            ParseErrorInfo::LexerError(r) => ("Invalid or unsupported syntax", r, None),
            ParseErrorInfo::RestrictedSyntax(r, kw) => {
                bare_search = true;
                ("Restricted syntax", r, Some(kw))
            }
        };
        let mut start_marker = self.prev_byte;
        let mut end_marker = self.byte;
        if let Some(keyword) = keyword {
            if let Some((start, end)) =
                locate_keyword(source, keyword, start_marker, end_marker, bare_search)
            {
                end_marker = start_marker + end;
                start_marker += start;
            }
        }

        match &self.error {
            ParseErrorInfo::LexerError(_) => {
                start_marker = self.byte - 1;
                end_marker = self.byte;
            }
            _ => (),
        }
        let marker = SourceAnnotation {
            label: reason,
            annotation_type: AnnotationType::Error,
            range: (start_marker, end_marker),
        };
        let title = Annotation {
            label: Some(err_type),
            id: None,
            annotation_type: AnnotationType::Error,
        };
        let snippet = Snippet {
            title: Some(title),
            footer: vec![],
            slices: vec![Slice {
                source: source,
                line_start: 1,
                origin: Some(filename),
                fold: true,
                annotations: vec![marker],
            }],
            opt: FormatOptions {
                color: true,
                ..Default::default()
            },
        };
        let list = DisplayList::from(snippet);
        list.to_string()
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
            ParseErrorInfo::InvalidSyntax(r) => ("Invalid syntax", r),
            ParseErrorInfo::ContextError(r, _) => ("Context error", r),
            ParseErrorInfo::SubstitutionError(r, _) => ("Substitution error", r),
            ParseErrorInfo::GlobError(r) => ("Glob translation error", r),
            ParseErrorInfo::RegexError(r) => ("Regex error", r),
            ParseErrorInfo::LexerError(r) => ("Invalid or unsupported syntax", r),
            ParseErrorInfo::RestrictedSyntax(r, _) => ("Restricted syntax", r),
        };

        write!(
            f,
            "{} at line {}, col {}. Reason: {}",
            err_type, self.line, self.col, reason
        )
    }
}

impl std::error::Error for ParseError {}
