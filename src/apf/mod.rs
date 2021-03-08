mod glob;
mod substitution;

use aho_corasick::{AhoCorasickBuilder, MatchKind};
use annotate_snippets::{
    display_list::{DisplayList, FormatOptions},
    snippet::{Annotation, AnnotationType, Slice, Snippet, SourceAnnotation},
};
use conch_parser::ast;
use conch_parser::lexer::Lexer;
use conch_parser::parse::DefaultParser;
use std::{collections::HashMap, fmt};

type Context = HashMap<String, String>;

#[derive(Debug)]
pub struct ParseError {
    line: usize,
    col: usize,
    byte: usize,
    prev_byte: usize,
    error: ParseErrorInfo,
}

#[derive(Debug)]
pub enum ParseErrorInfo {
    LexerError(String),
    InvalidSyntax(String),
    ContextError(String, String),
    SubstitutionError(String, String),
    GlobError(String),
    RegexError(String),
}

#[inline]
fn locate_keyword(source: &str, keyword: &str, start: usize, end: usize) -> Option<(usize, usize)> {
    let search = AhoCorasickBuilder::new()
        .match_kind(MatchKind::LeftmostLongest)
        .build(&[
            format!("${{{}", keyword).as_str(),
            format!("${}", keyword).as_str(),
        ]);
    if start > end {
        return None;
    }
    let range = search.find(&source.as_bytes()[start..end]);
    if let Some(range) = range {
        return Some((range.start(), range.end()));
    }

    None
}

macro_rules! var_name {
    ($name:ident) => {{
        if let ast::Parameter::Var(name) = $name {
            name.to_owned()
        } else {
            $name.to_string()
        }
    }};
}

impl ParseError {
    pub fn pretty_print(&self, source: &str, filename: &str) -> String {
        let (err_type, reason, keyword) = match &self.error {
            ParseErrorInfo::InvalidSyntax(r) => ("Invalid syntax", r, None),
            ParseErrorInfo::ContextError(r, kw) => ("Context error", r, Some(kw)),
            ParseErrorInfo::SubstitutionError(r, kw) => ("Substitution error", r, Some(kw)),
            ParseErrorInfo::GlobError(r) => ("Glob translation error", r, None),
            ParseErrorInfo::RegexError(r) => ("Regex error", r, None),
            ParseErrorInfo::LexerError(r) => ("Invalid or unsupported syntax", r, None),
        };
        let mut start_marker = self.prev_byte;
        let mut end_marker = self.byte;
        if let Some(keyword) = keyword {
            if let Some((start, end)) = locate_keyword(source, keyword, start_marker, end_marker) {
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
        };

        write!(
            f,
            "{} at line {}, col {}. Reason: {}",
            err_type, self.line, self.col, reason
        )
    }
}

impl std::error::Error for ParseError {}

pub fn parse(c: &str, context: &mut Context) -> Result<(), ParseError> {
    let lex = Lexer::new(c.chars());
    let mut parser = DefaultParser::new(lex);

    loop {
        let prev_pos = parser.pos();
        let cmd = match parser.complete_command() {
            Ok(x) => x,
            Err(e) => {
                let pos = parser.pos();
                return Err(ParseError {
                    line: pos.line,
                    col: pos.col,
                    byte: pos.byte,
                    prev_byte: prev_pos.byte,
                    error: ParseErrorInfo::LexerError(e.to_string()),
                });
            }
        };

        match cmd {
            Some(cmd) => {
                match get_args_top_level(&cmd, context) {
                    Ok(_) => (),
                    Err(e) => {
                        let pos = parser.pos();
                        return Err(ParseError {
                            line: pos.line,
                            col: pos.col,
                            byte: pos.byte,
                            prev_byte: prev_pos.byte,
                            error: e,
                        });
                    }
                };
            }
            None => {
                break;
            }
        }
    }

    Ok(())
}

fn get_args_top_level(
    cmd: &ast::TopLevelCommand<String>,
    context: &mut Context,
) -> Result<(), ParseErrorInfo> {
    match &cmd.0 {
        ast::Command::List(list) => {
            let results: Vec<Result<(), ParseErrorInfo>> = std::iter::once(&list.first)
                .chain(list.rest.iter().map(|and_or| match and_or {
                    ast::AndOr::And(cmd) | ast::AndOr::Or(cmd) => cmd,
                }))
                .map(|cmd| get_args_listable(&cmd, context))
                .collect();
            for r in results {
                match r {
                    Ok(_) => (),
                    Err(e) => {
                        return Err(e);
                    }
                }
            }
            Ok(())
        }
        ast::Command::Job(_l) => Err(ParseErrorInfo::InvalidSyntax(
            "Syntax error: job not allowed.".to_string(),
        )),
    }
}

fn get_args_listable(
    cmd: &ast::DefaultListableCommand,
    context: &mut Context,
) -> Result<(), ParseErrorInfo> {
    match cmd {
        ast::ListableCommand::Single(cmd) => get_args_pipeable(cmd, context),
        ast::ListableCommand::Pipe(_, _cmds) => Err(ParseErrorInfo::InvalidSyntax(
            "Pipe not allowed".to_string(),
        )),
    }
}

fn get_args_pipeable(
    cmd: &ast::DefaultPipeableCommand,
    context: &mut Context,
) -> Result<(), ParseErrorInfo> {
    match cmd {
        ast::PipeableCommand::Simple(cmd) => get_args_simple(cmd, context),
        ast::PipeableCommand::Compound(_cmd) => Err(ParseErrorInfo::InvalidSyntax(
            "Redirection, `if` or `for` are not allowed.".to_string(),
        )),
        ast::PipeableCommand::FunctionDef(_, _cmd) => Err(ParseErrorInfo::InvalidSyntax(
            "Function definition not allowed.".to_string(),
        )),
    }
}

fn get_args_simple(
    cmd: &ast::DefaultSimpleCommand,
    context: &mut Context,
) -> Result<(), ParseErrorInfo> {
    if !cmd.redirects_or_cmd_words.is_empty() {
        return Err(ParseErrorInfo::InvalidSyntax(
            "Commands not allowed.".to_string(),
        ));
    }

    // Find redirects. If found, return syntax error.
    if cmd
        .redirects_or_env_vars
        .iter()
        .any(|redirect_or_word| match redirect_or_word {
            ast::RedirectOrEnvVar::EnvVar(_name, _value) => false,
            ast::RedirectOrEnvVar::Redirect(_) => true,
        })
    {
        return Err(ParseErrorInfo::InvalidSyntax(
            "Redirects not allowed.".to_string(),
        ));
    }

    for redirect_or_env_var in cmd.redirects_or_env_vars.iter() {
        match redirect_or_env_var {
            ast::RedirectOrEnvVar::EnvVar(name, word) => {
                let word = match word {
                    Some(w) => w,
                    None => {
                        return Err(ParseErrorInfo::InvalidSyntax(format!(
                            "Variable {} assigned without value.",
                            name
                        )));
                    }
                };

                let value = get_complex_word_as_string(word, context)?;
                context.insert(name.to_string(), value);
            }
            ast::RedirectOrEnvVar::Redirect(_) => {
                return Err(ParseErrorInfo::InvalidSyntax(
                    "Redirection, `if` or `for` are not allowed.".to_string(),
                ));
            }
        };
    }
    Ok(())
}

fn get_complex_word_as_string(
    word: &ast::DefaultComplexWord,
    context: &Context,
) -> Result<String, ParseErrorInfo> {
    let word = match word {
        ast::ComplexWord::Single(word) => word.clone(),
        ast::ComplexWord::Concat(words) => {
            let mut word_content = String::new();
            for w in words {
                word_content += &get_word_as_string(w, context)?;
            }
            ast::Word::Simple(ast::SimpleWord::Literal(word_content))
        }
    };

    get_word_as_string(&word, context)
}

fn get_word_as_string(
    word: &ast::DefaultWord,
    context: &Context,
) -> Result<String, ParseErrorInfo> {
    let result = match word {
        ast::Word::SingleQuoted(w) => w.to_string(),
        ast::Word::Simple(w) => get_simple_word_as_string(w, context)?,
        ast::Word::DoubleQuoted(words) => {
            let mut value = String::new();
            for w in words {
                value += &get_simple_word_as_string(w, context)?;
            }
            value
        }
    };

    Ok(result)
}

fn get_simple_word_as_string(
    word: &ast::DefaultSimpleWord,
    context: &Context,
) -> Result<String, ParseErrorInfo> {
    match word {
        ast::SimpleWord::Literal(w) => Ok(w.to_string()),
        ast::SimpleWord::Escaped(w) => {
            let res = match w.as_str() {
                "\n" => "".to_string(),
                _ => w.to_string(),
            };
            Ok(res)
        }
        ast::SimpleWord::Colon => Ok(":".to_string()),
        ast::SimpleWord::Param(p) => match get_parameter_as_string(p, context)? {
            Some(p) => Ok(p),
            None => Err(ParseErrorInfo::ContextError(
                format!("variable '{}' is undefined", var_name!(p)),
                var_name!(p),
            )),
        },
        ast::SimpleWord::Subst(s) => get_subst_result(s, context),
        ast::SimpleWord::Star => Ok("*".to_string()),
        ast::SimpleWord::Question => Ok("?".to_string()),
        ast::SimpleWord::SquareOpen => Ok("[".to_string()),
        ast::SimpleWord::SquareClose => Ok("]".to_string()),
        ast::SimpleWord::Tilde => Ok("~".to_string()),
    }
}

fn get_parameter_as_string(
    parameter: &ast::DefaultParameter,
    context: &Context,
) -> Result<Option<String>, ParseErrorInfo> {
    match parameter {
        ast::Parameter::Var(name) => match context.get(name) {
            Some(value) => Ok(Some(value.clone())),
            None => Ok(None),
        },
        _ => Err(ParseErrorInfo::InvalidSyntax(
            "Unsupported parameter type.".to_string(),
        )),
    }
}

fn get_subst_origin(
    param: &ast::DefaultParameter,
    context: &Context,
) -> Result<String, ParseErrorInfo> {
    let origin = match get_parameter_as_string(param, context)? {
        Some(p) => p,
        None => {
            return Err(ParseErrorInfo::ContextError(
                format!("variable '{}' is undefined", var_name!(param)),
                var_name!(param),
            ));
        }
    };
    Ok(origin)
}

fn get_subst_result(
    subst: &ast::DefaultParameterSubstitution,
    context: &Context,
) -> Result<String, ParseErrorInfo> {
    match subst {
        ast::ParameterSubstitution::ReplaceString(param, command) => {
            let origin = get_subst_origin(param, context)?;
            let command = match command {
                Some(c) => get_complex_word_as_string(c, context)?,
                None => {
                    return Err(ParseErrorInfo::SubstitutionError(
                        "No substring command provided".to_string(),
                        var_name!(param),
                    ));
                }
            };

            substitution::get_replace(&origin, &command, false)
        }
        ast::ParameterSubstitution::ReplaceStringAll(param, command) => {
            let origin = get_subst_origin(param, context)?;
            let command = match command {
                Some(c) => get_complex_word_as_string(c, context)?,
                None => {
                    return Err(ParseErrorInfo::SubstitutionError(
                        "No substring command provided".to_string(),
                        var_name!(param),
                    ));
                }
            };
            substitution::get_replace(&origin, &command, true)
        }
        ast::ParameterSubstitution::Substring(param, command) => {
            let origin = get_subst_origin(param, context)?;
            let command = match command {
                Some(c) => get_complex_word_as_string(c, context)?,
                None => {
                    return Err(ParseErrorInfo::SubstitutionError(
                        "No substring command provided".to_string(),
                        var_name!(param),
                    ));
                }
            };

            substitution::get_substring(&origin, &command)
        }
        ast::ParameterSubstitution::Error(colon, param, command) => {
            let origin = get_subst_origin(param, context);
            if let Ok(origin) = origin {
                if !colon || !origin.is_empty() {
                    return Ok(origin);
                }
            }
            let command = match command {
                Some(c) => get_complex_word_as_string(c, context)?,
                None => {
                    return Err(ParseErrorInfo::SubstitutionError(
                        "No error message provided".to_string(),
                        var_name!(param),
                    ));
                }
            };

            Err(ParseErrorInfo::SubstitutionError(
                format!("{} undefined: {}", param, command),
                var_name!(param),
            ))
        }
        ast::ParameterSubstitution::Len(param) => {
            let origin = get_subst_origin(param, context)?;

            Ok(format!("{}", origin.len()))
        }
        ast::ParameterSubstitution::Command(_) => Err(ParseErrorInfo::InvalidSyntax(
            "Command substitution is not allowed.".to_string(),
        )),
        ast::ParameterSubstitution::Default(colon, param, command) => {
            let origin = get_subst_origin(param, context);
            if let Ok(origin) = origin {
                if !colon || !origin.is_empty() {
                    return Ok(origin);
                }
            }

            let command = match command {
                Some(c) => get_complex_word_as_string(c, context)?,
                None => {
                    return Err(ParseErrorInfo::SubstitutionError(
                        "No default value provided".to_string(),
                        var_name!(param),
                    ));
                }
            };

            Ok(command)
        }
        ast::ParameterSubstitution::Alternative(colon, param, command) => {
            let origin = get_subst_origin(param, context);
            match origin {
                Ok(origin) => {
                    if *colon && origin.is_empty() {
                        return Ok(String::new());
                    }
                }
                Err(_) => return Ok(String::new()),
            }

            let command = match command {
                Some(c) => get_complex_word_as_string(c, context)?,
                None => {
                    return Err(ParseErrorInfo::SubstitutionError(
                        "No alternative value provided".to_string(),
                        var_name!(param),
                    ));
                }
            };

            Ok(command)
        }
        ast::ParameterSubstitution::RemoveSmallestPrefix(param, command) => {
            let origin = get_subst_origin(param, context)?;
            let command = match command {
                Some(c) => get_complex_word_as_string(c, context)?,
                None => {
                    return Err(ParseErrorInfo::SubstitutionError(
                        "No substring command provided".to_string(),
                        var_name!(param),
                    ));
                }
            };

            substitution::get_trim_prefix(&origin, &command, true, false)
        }
        ast::ParameterSubstitution::RemoveLargestPrefix(param, command) => {
            let origin = get_subst_origin(param, context)?;
            let command = match command {
                Some(c) => get_complex_word_as_string(c, context)?,
                None => {
                    return Err(ParseErrorInfo::SubstitutionError(
                        "No substring command provided".to_string(),
                        var_name!(param),
                    ));
                }
            };

            substitution::get_trim_prefix(&origin, &command, true, true)
        }
        ast::ParameterSubstitution::RemoveSmallestSuffix(param, command) => {
            let origin = get_subst_origin(param, context)?;
            let command = match command {
                Some(c) => get_complex_word_as_string(c, context)?,
                None => {
                    return Err(ParseErrorInfo::SubstitutionError(
                        "No substring command provided".to_string(),
                        var_name!(param),
                    ));
                }
            };

            substitution::get_trim_prefix(&origin, &command, false, false)
        }
        ast::ParameterSubstitution::RemoveLargestSuffix(param, command) => {
            let origin = get_subst_origin(param, context)?;
            let command = match command {
                Some(c) => get_complex_word_as_string(c, context)?,
                None => {
                    return Err(ParseErrorInfo::SubstitutionError(
                        "No substring command provided".to_string(),
                        var_name!(param),
                    ));
                }
            };

            substitution::get_trim_prefix(&origin, &command, false, true)
        }
        ast::ParameterSubstitution::Lowercase(all, param, command) => {
            let origin = get_subst_origin(param, context)?;
            let command = match command {
                Some(c) => get_complex_word_as_string(c, context)?,
                None => {
                    return substitution::get_lower_case(&origin, None, *all);
                }
            };

            substitution::get_lower_case(&origin, Some(&command), *all)
        }
        ast::ParameterSubstitution::Uppercase(all, param, command) => {
            let origin = get_subst_origin(param, context)?;
            let command = match command {
                Some(c) => get_complex_word_as_string(c, context)?,
                None => {
                    return substitution::get_upper_case(&origin, None, *all);
                }
            };

            substitution::get_upper_case(&origin, Some(&command), *all)
        }
        ast::ParameterSubstitution::Assign(_, param, _) => Err(ParseErrorInfo::SubstitutionError(
            format!(
                "Variable assignment ({}) inside a substitution is not allowed",
                param
            ),
            var_name!(param),
        )),
        ast::ParameterSubstitution::Arith(command) => Err(ParseErrorInfo::SubstitutionError(
            format!(
                "Arithmetic operation ({:?}) inside a substitution is not supported",
                command
            ),
            "((".to_string(),
        )),
    }
}
