//! Recursive-descent parser for the apml language.
//!
//! The language is a strict Bash subset: only variable assignments, comments
//! and blank lines are allowed at the top level.

use super::ast::*;
use super::error::{Diagnostic, DiagnosticInfo, ParseErrorInfo};
use super::lexer::{Lexer, Token};

/// Parse a whole source file into a list of statements.
///
/// On syntax errors, all errors in the file are collected and returned.
pub fn parse_program(src: &str) -> Result<Vec<Stmt>, Vec<Diagnostic>> {
    let mut lexer = Lexer::new(src);
    let mut stmts = Vec::new();
    let mut errors = Vec::new();

    loop {
        let tok = match lexer.next_token() {
            Ok(t) => t,
            Err(e) => {
                let pos = lexer.span();
                errors.push(Diagnostic {
                    span: pos.into(),
                    info: DiagnosticInfo::Error(e),
                });
                lexer.recover_to_newline();
                continue;
            }
        };

        match tok {
            Token::Eof => break,
            Token::Newline => continue,
            Token::Name(name) => {
                let name_span = lexer.last_span();
                let op = match lexer.next_token() {
                    Ok(Token::Equals) => AssignOp::Eq,
                    Ok(Token::PlusEquals) => AssignOp::PlusEq,
                    Ok(Token::Newline) | Ok(Token::Eof) => {
                        errors.push(Diagnostic {
                            span: name_span.into(),
                            info: DiagnosticInfo::Error(ParseErrorInfo::RestrictedSyntax(
                                format!("Variable {} assigned without value.", name),
                                name,
                            )),
                        });
                        continue;
                    }
                    Ok(_) => {
                        errors.push(Diagnostic {
                            span: name_span.into(),
                            info: DiagnosticInfo::Error(ParseErrorInfo::InvalidSyntax(
                                "Expected `=` or `+=` after variable name.".to_string(),
                            )),
                        });
                        lexer.recover_to_newline();
                        continue;
                    }
                    Err(e) => {
                        let pos = lexer.span();
                        errors.push(Diagnostic {
                            span: pos.into(),
                            info: DiagnosticInfo::Error(e),
                        });
                        lexer.recover_to_newline();
                        continue;
                    }
                };

                let value = match lexer.peek() {
                    Some('(') => match lexer.scan_array() {
                        Ok(elems) => ValueExpr::Array(elems),
                        Err(e) => {
                            let pos = lexer.span();
                            errors.push(Diagnostic {
                                span: pos.into(),
                                info: DiagnosticInfo::Error(e),
                            });
                            lexer.recover_to_newline();
                            continue;
                        }
                    },
                    Some('\n') | None => ValueExpr::Scalar(Word {
                        span: name_span,
                        end: name_span.byte,
                        fields: Vec::new(),
                    }),
                    _ => match lexer.scan_word(false) {
                        Ok(w) => ValueExpr::Scalar(w),
                        Err(e) => {
                            let pos = lexer.span();
                            errors.push(Diagnostic {
                                span: pos.into(),
                                info: DiagnosticInfo::Error(e),
                            });
                            lexer.recover_to_newline();
                            continue;
                        }
                    },
                };

                stmts.push(Stmt {
                    span: name_span,
                    name,
                    op,
                    value,
                });
            }
            Token::Word(_) => {
                let pos = lexer.last_span();
                errors.push(Diagnostic {
                    span: pos.into(),
                    info: DiagnosticInfo::Error(ParseErrorInfo::InvalidSyntax(
                        "Commands are not allowed.".to_string(),
                    )),
                });
                lexer.recover_to_newline();
            }
            Token::Equals | Token::PlusEquals => {
                let pos = lexer.last_span();
                errors.push(Diagnostic {
                    span: pos.into(),
                    info: DiagnosticInfo::Error(ParseErrorInfo::InvalidSyntax(
                        "Unexpected `=`.".to_string(),
                    )),
                });
                lexer.recover_to_newline();
            }
        }
    }

    if errors.is_empty() {
        Ok(stmts)
    } else {
        Err(errors)
    }
}
