use std::path::PathBuf;

use anyhow::{format_err, Result};
use std::collections::HashMap;
use conch_parser::ast;
use conch_parser::lexer::Lexer;
use conch_parser::parse::DefaultParser;

type Context = HashMap<String, String>;

#[derive(Debug)]
pub struct ParseError {
    line: usize,
    col: usize,
    error: ParseErrorInfo
}

#[derive(Debug)]
pub enum ParseErrorInfo {
    InvalidSyntax(String),
    ContextError(String)
}

pub fn parse(c: &str, context: &mut Context) -> Result<(), ParseError> {
    let lex = Lexer::new(c.chars());
    let mut parser = DefaultParser::new(lex);

    loop {
        let cmd = match parser.complete_command() {
            Ok(x) => x,
            Err(e) => {
                let pos = parser.pos();
                return Err(ParseError {
                    line: pos.line,
                    col: pos.col,
                    error: ParseErrorInfo::InvalidSyntax(e.to_string()),
                });
            },
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
                            error: e
                        })
                    }
                };
            },
            None => {break;}
        }
    }

    Ok(())
}

fn get_args_top_level(cmd: &ast::TopLevelCommand<String>, context: &mut Context) -> Result<(), ParseErrorInfo> {
    match &cmd.0 {
        ast::Command::List(list) => {
            let results: Vec<Result<(), ParseErrorInfo>> = std::iter::once(&list.first)
                .chain(list.rest.iter().map(|and_or| match and_or {
                    ast::AndOr::And(cmd) | ast::AndOr::Or(cmd) => cmd,
                }))
                .map(|cmd| get_args_listable(&cmd, context))
                .collect();
            println!("{:?}", results);
            for r in results {
                match r {
                    Ok(_) => (),
                    Err(e) => { return Err(e); }, 
                }
            }
            Ok(())
        }
        ast::Command::Job(_l) => {
            Err(ParseErrorInfo::InvalidSyntax("Syntax error: job not allowed.".to_string()))
        }
    }
}

fn get_args_listable(cmd: &ast::DefaultListableCommand, context: &mut Context) -> Result<(), ParseErrorInfo> {
    match cmd {
        ast::ListableCommand::Single(cmd) => get_args_pipeable(cmd, context),
        ast::ListableCommand::Pipe(_, _cmds) => {
            Err(ParseErrorInfo::InvalidSyntax("Pipe not allowed".to_string()))
        }
    }
}

fn get_args_pipeable(cmd: &ast::DefaultPipeableCommand, context: &mut Context) -> Result<(), ParseErrorInfo> {
    match cmd {
        ast::PipeableCommand::Simple(cmd) => get_args_simple(cmd, context),
        ast::PipeableCommand::Compound(_cmd) => {
            Err(ParseErrorInfo::InvalidSyntax("Redirection not allowed.".to_string()))
        }
        ast::PipeableCommand::FunctionDef(_, _cmd) => {
            Err(ParseErrorInfo::InvalidSyntax("Function definition not allowed.".to_string()))
        }
    }
}

fn get_args_simple(cmd: &ast::DefaultSimpleCommand, context: &mut Context) -> Result<(), ParseErrorInfo> {
    if !cmd.redirects_or_cmd_words.is_empty() {
        return Err(ParseErrorInfo::InvalidSyntax("Commands not allowed.".to_string()));
    }

    // Find redirects. If found, return syntax error.
    if cmd
        .redirects_or_env_vars
        .iter()
        .find(|redirect_or_word| match redirect_or_word {
            ast::RedirectOrEnvVar::EnvVar(_name, _value) => false,
            ast::RedirectOrEnvVar::Redirect(_) => true,
        })
        .is_some()
    {
        return Err(ParseErrorInfo::InvalidSyntax("Redirects not allowed.".to_string()));
    }

    for redirect_or_env_var in cmd.redirects_or_env_vars.iter() {
        match redirect_or_env_var {
            ast::RedirectOrEnvVar::EnvVar(name, word) => {
                let word = match word {
                    Some(w) => w,
                    None => {
                        return Err(ParseErrorInfo::InvalidSyntax(format!("Variable {} without value.", name)));
                    }
                };

                get_vec_value(word, name, context)?
            },
            ast::RedirectOrEnvVar::Redirect(_) => {
                return Err(ParseErrorInfo::InvalidSyntax("Redirects not allowed.".to_string()));
            },
        };
    }
    Ok(())
}

fn get_vec_value(word: &ast::DefaultComplexWord, name: &str, context: &mut Context) -> Result<(), ParseErrorInfo> {
    let word = match word {
        ast::ComplexWord::Single(w) => w,
        ast::ComplexWord::Concat(_w) => {
            // TODO: What is a concatanated word?
            return Err(ParseErrorInfo::InvalidSyntax("Concatanated word unsupported.".to_string()));
        }
    };

    match word {
        ast::Word::SingleQuoted(w) => {
            context.insert(name.to_string(), w.to_string());
        },
        ast::Word::Simple(w) => {
            let value = get_simple_word_as_string(w, context)?.to_string();
            context.insert(name.to_string(), value.to_string());
        },
        ast::Word::DoubleQuoted(words) => {
            let mut value = String::new();
            for w in words {
                value += &get_simple_word_as_string(w, context)?;
            }
            context.insert(name.to_string(), value.to_string());
        },
    };

    Ok(())
}

fn get_simple_word_as_string(word: &ast::DefaultSimpleWord, context: &mut Context) -> Result<String, ParseErrorInfo> {
    match word {
        ast::SimpleWord::Literal(w) => Ok(w.to_string()),
        ast::SimpleWord::Escaped(w) => {
            let res = match w.as_str() {
                "\n" => "".to_string(),
                _ => w.to_string(),
            };
            Ok(res)
        },
        ast::SimpleWord::Param(p) => {
            match get_parameter_as_string(p, context)? {
                Some(p) => Ok(p),
                None => Err(ParseErrorInfo::ContextError("Param variable not found.".to_string()))
            }
        },
        //ast::SimpleWord::Subst(s) => Ok(s), // TODO
        _ => Err(ParseErrorInfo::InvalidSyntax("Encountered star, square, tide, or other unsupported chatacters.".to_string())),
    }
}

fn get_parameter_as_string(parameter: &ast::DefaultParameter, context: &mut Context) -> Result<Option<String>, ParseErrorInfo> {
    match parameter {
        ast::Parameter::Var(name) => {
            match context.get(name) {
                Some(value) => {
                    Ok(Some(value.clone()))
                },
                None => {
                    Ok(None)
                }
            }
        },
        _ => Err(ParseErrorInfo::InvalidSyntax("Unsupported parameter.".to_string()))
    }
}

fn get_subst_result(subst: &ast::DefaultParameterSubstitution) -> Result<String> {
    todo!();
    /*
    match subst {
        &ast::ParameterSubstitution::Substring()
    }
    */
}
