use std::path::PathBuf;

use anyhow::{format_err, Result};
use std::collections::HashMap;
use conch_parser::ast;
use conch_parser::lexer::Lexer;
use conch_parser::parse::DefaultParser;

type Context = HashMap<String, String>;

pub fn try_parse(c: &str, context: &mut Context) -> Result<()> {
    let lex = Lexer::new(c.chars());
    let parser = DefaultParser::new(lex);

    for cmd in parser {
        get_args_top_level(&cmd?, context)?;
    }

    Ok(())
}

fn get_args_top_level(cmd: &ast::TopLevelCommand<String>, context: &mut Context) -> Result<()> {
    match &cmd.0 {
        ast::Command::List(list) => {
            let results: Vec<Result<()>> = std::iter::once(&list.first)
                .chain(list.rest.iter().map(|and_or| match and_or {
                    ast::AndOr::And(cmd) | ast::AndOr::Or(cmd) => cmd,
                }))
                .map(|cmd| get_args_listable(&cmd, context))
                .collect();
            println!("{:?}", results);
            return Ok(());
        }
        ast::Command::Job(_l) => {
            return Err(format_err!("Syntax error: job not allowed."));
        }
    }
}

fn get_args_listable(cmd: &ast::DefaultListableCommand, context: &mut Context) -> Result<()> {
    match cmd {
        ast::ListableCommand::Single(cmd) => get_args_pipeable(cmd, context),
        ast::ListableCommand::Pipe(_, _cmds) => {
            return Err(format_err!("Syntax error: pipe not allowed."));
        }
    }
}

fn get_args_pipeable(cmd: &ast::DefaultPipeableCommand, context: &mut Context) -> Result<()> {
    match cmd {
        ast::PipeableCommand::Simple(cmd) => get_args_simple(cmd, context),
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

fn get_args_simple(cmd: &ast::DefaultSimpleCommand, context: &mut Context) -> Result<()> {
    if !cmd.redirects_or_cmd_words.is_empty() {
        return Err(format_err!("Syntax error: commands not allowed."));
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

                get_vec_value(word, name, context)?
            },
            ast::RedirectOrEnvVar::Redirect(_) => {
                return Err(format_err!("Syntax error: redirects not allowed."));
            },
        };
    }
    Ok(())
}

fn get_vec_value(word: &ast::DefaultComplexWord, name: &str, context: &mut Context) -> Result<()> {
    let word = match word {
        ast::ComplexWord::Single(w) => w,
        ast::ComplexWord::Concat(_w) => {
            // TODO: What is a concatanated word?
            return Err(format_err!("Syntax error: concatanated word unsupported."))
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

fn get_simple_word_as_string(word: &ast::DefaultSimpleWord, context: &mut Context) -> Result<String> {
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
                None => Err(format_err!("Context error: variable not found."))
            }
        },
        //ast::SimpleWord::Subst(s) => Ok(s), // TODO
        _ => Err(format_err!("Syntax error: encountered star, square, tide, or other unsupported chatacters.")),
    }
}

fn get_parameter_as_string(parameter: &ast::DefaultParameter, context: &mut Context) -> Result<Option<String>> {
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
        _ => Err(format_err!("Syntax error: unsupported parameter."))
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
