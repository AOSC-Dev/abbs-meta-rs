//! AST for the ACBS Package Metadata Language (a strict Bash subset).

/// A position in the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// Byte offset in the source.
    pub byte: usize,
    /// 1-based line.
    pub line: usize,
    /// 1-based column.
    pub col: usize,
}

/// A parsed statement: `NAME=value` or `NAME+=(...)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stmt {
    pub span: Span,
    pub name: String,
    pub op: AssignOp,
    pub value: ValueExpr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    /// `=`
    Eq,
    /// `+=`
    PlusEq,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueExpr {
    /// A scalar value (possibly empty).
    Scalar(Word),
    /// An array literal `( ... )`.
    Array(Vec<Word>),
}

/// A shell word: a concatenation of fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub span: Span,
    pub fields: Vec<Field>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Field {
    Literal(String),
    /// An unquoted escaped character `\c` (the backslash is removed, but the
    /// escape information is preserved — needed e.g. to distinguish `~` from
    /// `\~` for tilde expansion in `${var/pat/repl}`).
    Escaped(char),
    SingleQuoted(String),
    DoubleQuoted(Vec<Field>),
    Param(Param),
    /// `$( ... )` — one or more pipelines of simple commands.
    Command(Vec<Command>),
    /// `$(( ... ))` — raw arithmetic content.
    Arith(String),
}

/// A command substitution `$( ... )`: one or more pipelines, each a list of
/// stages (a stage is a list of words).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// Pipeline stages, e.g. `echo $X | cut -d . -f2` → two stages.
    pub stages: Vec<Vec<Word>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Param {
    /// `$NAME`
    Plain {
        name: String,
        /// Byte offset of the variable name in the source (right after `$`).
        name_start: usize,
    },
    /// `$@` `$*` `$#` `$?` `$-` `$$` `$!`
    Special(char),
    /// `$0` .. `$9`, `${10}`
    Positional(u32),
    /// `${NAME...}` (or `${NAME[index]...}`)
    Braced {
        name: String,
        /// Byte offset of the variable name in the source (right after `${`).
        name_start: usize,
        index: Option<Index>,
        op: BracedOp,
    },
    /// `${#NAME}` / `${#NAME[@]}`
    Length {
        name: String,
        /// Byte offset of the variable name in the source (right after `${#`).
        name_start: usize,
        index: Option<Index>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Index {
    At,
    Star,
    Number(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BracedOp {
    /// No operator: plain value expansion.
    Value,
    /// `${name:-word}` / `${name-word}`
    Default { colon: bool, word: Word },
    /// `${name:=word}` / `${name=word}`
    Assign { colon: bool, word: Word },
    /// `${name:?word}` / `${name?word}`
    Error { colon: bool, word: Word },
    /// `${name:+word}` / `${name+word}`
    Alternative { colon: bool, word: Word },
    /// `${name:off[:len]}`
    Substring(Word),
    /// `${name#w}` `${name##w}` `${name%w}` `${name%%w}`
    Trim { kind: TrimKind, word: Word },
    /// `${name/pat/rep}` `${name//pat/rep}` `${name/#pat/rep}` `${name/%pat/rep}`
    Replace {
        all: bool,
        anchor: ReplaceAnchor,
        pattern: Word,
        replacement: Word,
    },
    /// `${name^pat}` `${name^^pat}` `${name,pat}` `${name,,pat}`
    CaseMod { kind: CaseKind, word: Word },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrimKind {
    PrefixShort,
    PrefixLong,
    SuffixShort,
    SuffixLong,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplaceAnchor {
    Any,
    Prefix,
    Suffix,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaseKind {
    UpperOnce,
    UpperAll,
    LowerOnce,
    LowerAll,
}
