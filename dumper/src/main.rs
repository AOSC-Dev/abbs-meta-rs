//! abbs-meta-dump — parse a whole aosc-os-abbs tree and dump variables.
//!
//! Subcommands:
//!   (default)          dump spec + defines variables to
//!                      `/tmp/all_vars_rs.json` / `/tmp/all_vars_def_rs.json`
//!   categorize [DIR]   categorize parse errors by kind and reason
//!   filecat [DIR]      attribute failing defines files to error categories
//!
//! The default dump requires `SPEC_DIR`; the subcommands accept a directory
//! argument or fall back to the `SPEC_DIR` environment variable.

use abbs_meta_apml::{parse, Context, ParseError, ParseErrorInfo, Value};
use anyhow::Result;
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};

const DUMMY_AB_IMPORT: &[&str] = &["SRCDIR", "PKGDIR", "PKGVER", "PKGREL", "ARCH"];

#[inline]
fn try_parse(content: &str, dummy_import: bool) -> Result<Context, Vec<ParseError>> {
    let mut context = Context::new();
    if dummy_import {
        for pred in DUMMY_AB_IMPORT {
            context.insert(pred.to_string(), Value::Scalar(String::new()));
        }
    }
    // Safe: `$( ... )` command substitutions are never executed — they
    // expand to an empty string in `parse()`.
    parse(content, &mut context)?;
    if dummy_import {
        for pred in DUMMY_AB_IMPORT {
            context.remove(&pred.to_string());
        }
    }

    Ok(context)
}

fn dump_whole_tree(is_spec: bool, dummy_import: bool) -> Result<String> {
    // Code for speed testing
    let spec_dir = std::env::var("SPEC_DIR")?;
    let print_errors = std::env::var("PRINT_ERROR").is_ok();
    let mut dump: HashMap<String, Context> = HashMap::new();
    let mut defines = Vec::new();
    let mut errors = 0usize;
    let mut total = 0usize;
    let walker = walkdir::WalkDir::new(&spec_dir).max_depth(4);
    for entry in walker.into_iter() {
        let file = entry?;
        if file.file_name() == {
            if is_spec {
                "spec"
            } else {
                "defines"
            }
        } {
            let path = PathBuf::from(file.path());
            defines.push(path);
        }
    }

    for p in defines.into_iter() {
        let mut f = File::open(&p).unwrap();
        let mut content = String::new();
        f.read_to_string(&mut content).unwrap();
        total += 1;
        let parse_result = try_parse(&content, dummy_import && !is_spec);
        if let Ok(context) = parse_result {
            let name = p.strip_prefix(&spec_dir)?;
            dump.insert(name.to_string_lossy().to_string(), context);
        } else {
            if print_errors {
                for result in parse_result.unwrap_err() {
                    println!("{}", result.pretty_print(&content, &p.to_string_lossy()));
                }
            }
            errors += 1;
        }
    }
    println!(
        "Total: {}, Errors: {} ({}%)",
        total,
        errors,
        errors * 100 / total
    );

    Ok(serde_json::to_string(&dump)?)
}

fn dump() -> Result<()> {
    println!("[ spec  ] Collecting variables ...");
    let dump = dump_whole_tree(true, true)?;
    let mut f = File::create("/tmp/all_vars_rs.json")?;
    f.write_all(dump.as_bytes())?;
    println!("[defines] Collecting variables ...");
    let dump = dump_whole_tree(false, true)?;
    let mut f = File::create("/tmp/all_vars_def_rs.json")?;
    f.write_all(dump.as_bytes())?;

    Ok(())
}

/// Collect all files named `target` under `spec_dir`.
fn collect_files(spec_dir: &str, target: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(spec_dir).max_depth(6) {
        let entry = entry.unwrap();
        if entry.file_name() == target {
            files.push(PathBuf::from(entry.path()));
        }
    }
    files
}

fn read_file(path: &PathBuf) -> String {
    let mut f = File::open(path).unwrap();
    let mut content = String::new();
    f.read_to_string(&mut content).unwrap();
    content
}

/// `categorize` subcommand: count parse errors by category and reason.
fn categorize(spec_dir: Option<String>) {
    let spec_dir = spec_dir
        .expect("usage: abbs-meta-dump categorize [SPEC_DIR] (or set SPEC_DIR)");
    for is_spec in [true, false] {
        let target = if is_spec { "spec" } else { "defines" };
        let files = collect_files(&spec_dir, target);

        let mut cats: HashMap<String, usize> = HashMap::new();
        let mut by_reason: HashMap<String, usize> = HashMap::new();
        let mut examples: HashMap<String, String> = HashMap::new();
        let mut total_err = 0usize;

        for p in files.iter() {
            let content = read_file(p);
            let mut ctx = Context::new();
            if let Err(errs) = parse(&content, &mut ctx) {
                total_err += 1;
                for e in errs {
                    let (cat, reason) = match &e.error {
                        ParseErrorInfo::LexerError(r) => ("LexerError".to_string(), r.clone()),
                        ParseErrorInfo::InvalidSyntax(r) => {
                            ("InvalidSyntax".to_string(), r.clone())
                        }
                        ParseErrorInfo::RestrictedSyntax(r, kw) => {
                            (format!("RestrictedSyntax({kw})"), r.clone())
                        }
                        ParseErrorInfo::ContextError(r, kw) => {
                            (format!("ContextError({kw})"), r.clone())
                        }
                        ParseErrorInfo::SubstitutionError(r, kw) => {
                            (format!("SubstitutionError({kw})"), r.clone())
                        }
                        ParseErrorInfo::GlobError(r) => ("GlobError".to_string(), r.clone()),
                        ParseErrorInfo::RegexError(r) => ("RegexError".to_string(), r.clone()),
                    };
                    *cats.entry(cat.clone()).or_insert(0) += 1;
                    *by_reason.entry(reason.clone()).or_insert(0) += 1;
                    examples
                        .entry(cat)
                        .or_insert_with(|| p.display().to_string());
                }
            }
        }

        println!(
            "=== {target}: {total_err} files with errors / {} total ===",
            files.len()
        );
        let mut cats: Vec<_> = cats.into_iter().collect();
        cats.sort_by(|a, b| b.1.cmp(&a.1));
        for (k, v) in cats {
            println!("  {v:>5}  {k:<50}  e.g. {}", examples[&k]);
        }
        let mut reasons: Vec<_> = by_reason.into_iter().collect();
        reasons.sort_by(|a, b| b.1.cmp(&a.1));
        println!("  ---- top reasons ----");
        for (k, v) in reasons.iter().take(25) {
            println!("  {v:>5}  {k}");
        }
    }
}

/// `filecat` subcommand: attribute each failing defines file to the error
/// categories it involves.
fn filecat(spec_dir: Option<String>) {
    let spec_dir = spec_dir.expect("usage: abbs-meta-dump filecat [SPEC_DIR] (or set SPEC_DIR)");
    let files = collect_files(&spec_dir, "defines");

    let cat_of = |e: &ParseErrorInfo| -> &'static str {
        match e {
            ParseErrorInfo::LexerError(_) => "lexer",
            ParseErrorInfo::InvalidSyntax(_) => "invalid",
            ParseErrorInfo::RestrictedSyntax(r, _) => {
                if r.contains("without value") {
                    "array-literal"
                } else {
                    "compound/redirect"
                }
            }
            ParseErrorInfo::ContextError(_, _) => "undefined-var",
            ParseErrorInfo::SubstitutionError(_, _) => "substitution",
            ParseErrorInfo::GlobError(_) => "glob",
            ParseErrorInfo::RegexError(_) => "regex",
        }
    };

    // For each file, collect the set of categories it fails with.
    let mut per_file: HashMap<&'static str, usize> = HashMap::new();
    let mut examples: HashMap<&'static str, String> = HashMap::new();
    let mut fail = 0usize;

    for p in files.iter() {
        let content = read_file(p);
        let mut ctx = Context::new();
        if let Err(errs) = parse(&content, &mut ctx) {
            fail += 1;
            let mut cats: Vec<&'static str> = errs.iter().map(|e| cat_of(&e.error)).collect();
            cats.sort();
            cats.dedup();
            for c in cats {
                *per_file.entry(c).or_insert(0) += 1;
                examples.entry(c).or_insert_with(|| p.display().to_string());
            }
        }
    }

    println!("Total failing defines files: {fail} / {}", files.len());
    let mut v: Vec<_> = per_file.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1));
    for (k, n) in v {
        println!("  {n:>5} files involve: {k:<20} e.g. {}", examples[&k]);
    }
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("categorize") => {
            categorize(args.next().or_else(|| std::env::var("SPEC_DIR").ok()));
            Ok(())
        }
        Some("filecat") => {
            filecat(args.next().or_else(|| std::env::var("SPEC_DIR").ok()));
            Ok(())
        }
        _ => dump(),
    }
}
