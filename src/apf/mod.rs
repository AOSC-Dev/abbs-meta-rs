use std::path::PathBuf;

use anyhow::{format_err, Result};
use conch_parser::ast;
use conch_parser::lexer::Lexer;
use conch_parser::parse::DefaultParser;

pub fn try_parse(c: &str) -> Result<()> {
    let lex = Lexer::new(c.chars());
    let parser = DefaultParser::new(lex);

    for cmd in parser {
        get_args_top_level(&cmd?)?;
    }

    Ok(())
}

type Args = Vec<(String, String)>;

fn get_args_top_level(cmd: &ast::TopLevelCommand<String>) -> Result<Args> {
    match &cmd.0 {
        ast::Command::List(list) => {
            let res: Vec<Result<Args>> = std::iter::once(&list.first)
                .chain(list.rest.iter().map(|and_or| match and_or {
                    ast::AndOr::And(cmd) | ast::AndOr::Or(cmd) => cmd,
                }))
                .map(|cmd| get_args_listable(&cmd))
                .collect();

            let mut args = Vec::new();
            for i in res {
                match i {
                    Ok(v) => {
                        args.extend(v);
                    }
                    Err(e) => {
                        return Err(e);
                    }
                }
            }

            return Ok(args);
        }
        ast::Command::Job(_l) => {
            return Err(format_err!("Syntax error: job not allowed."));
        }
    }
}

fn get_args_listable(cmd: &ast::DefaultListableCommand) -> Result<Args> {
    match cmd {
        ast::ListableCommand::Single(cmd) => get_args_pipeable(cmd),
        ast::ListableCommand::Pipe(_, _cmds) => {
            return Err(format_err!("Syntax error: pipe not allowed."));
        }
    }
}

fn get_args_pipeable(cmd: &ast::DefaultPipeableCommand) -> Result<Args> {
    match cmd {
        ast::PipeableCommand::Simple(cmd) => get_args_simple(cmd),
        ast::PipeableCommand::Compound(_cmd) => {
            return Err(format_err!("Syntax error: redirection not allowed."));
        }
        ast::PipeableCommand::FunctionDef(_, _cmd) => {
            return Err(format_err!(
                "Syntax error: function definition not allowed."
            ));
        }
    }
}

fn get_args_simple(cmd: &ast::DefaultSimpleCommand) -> Result<Args> {
    if !cmd.redirects_or_cmd_words.is_empty() {
        return Err(format_err!("Syntax error: commands not allowed."));
    }

    // Find redirects. If found, return syntax error.
    if cmd
        .redirects_or_env_vars
        .iter()
        .find(|redirect_or_word| match redirect_or_word {
            ast::RedirectOrEnvVar::EnvVar(_name, _value) => true,
            ast::RedirectOrEnvVar::Redirect(_) => false,
        })
        .is_some()
    {
        return Err(format_err!("Syntax error: redirects not allowed."));
    }

    for redirect_or_env_var in cmd.redirects_or_env_vars.iter() {
        match redirect_or_env_var {
            ast::RedirectOrEnvVar::EnvVar(name, word) => {
                let word = match word {
                    Some(w) => w,
                    None => {
                        return Err(format_err!("Syntax error: variable {} without value.", name));
                    }
                };

                let res = get_vec_value(word);

            },
            ast::RedirectOrEnvVar::Redirect(_) => {
                return Err(format_err!("Syntax error: redirects not allowed."));
            },
        };
    }
    todo!()
}

fn get_vec_value(word: &ast::DefaultComplexWord) -> Result<Vec<String>> {
    let word = match word {
        ast::ComplexWord::Single(w) => w,
        ast::ComplexWord::Concat(_w) => {
            // TODO: What is a concatanated word?
            return Err(format_err!("Syntax error: concatanated word unsupported."))
        }
    };

    match word {
        ast::Word::SingleQuoted(w) => Ok(vec!(w.clone())),
        ast::Word::Simple(w) => {
            Ok(vec!(get_simple_word_as_string(w)?.to_string()))
        },
        ast::Word::DoubleQuoted(words) => {
            todo!()
        },
    }
}



fn get_simple_word_as_string(word: &ast::DefaultSimpleWord) -> Result<&String> {
    match word {
        ast::SimpleWord::Literal(w) => Ok(w),
        ast::SimpleWord::Escaped(w) => Ok(w), // TODO: Is this up to syntax standard?

        ast::SimpleWord::Param(p) => Ok(p), // TODO: Should be similar to a subsitution
        ast::SimpleWord::Subst(s) => Ok(s), // TODO: Implement a type for this
        _ => Err(format_err!("Syntax error: unsupprted star, square, tide, or other chatacters.")), // Other types are 
    }
}
