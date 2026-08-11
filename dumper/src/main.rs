use abbs_meta_apml::{parse_with_runner, Context, ParseError, Value};
use anyhow::Result;
use std::{
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};

const DUMMY_AB_IMPORT: &[&str] = &["SRCDIR", "PKGDIR", "PKGVER", "PKGREL", "ARCH"];

/// Shell-quote a word for `sh -c` evaluation.
fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Run a `$( ... )` command substitution through `sh -c`.
///
/// This mirrors the reference implementation (bashd), which sources the
/// files in a real shell. Only enabled when the `CMD_SUBST` environment
/// variable is set.
fn run_sh(stages: &[Vec<String>]) -> Result<String, String> {
    let pipeline = stages
        .iter()
        .map(|words| {
            words
                .iter()
                .map(|w| sh_quote(w))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(" | ");
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(&pipeline)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("command failed: {pipeline}"));
    }
    // Bash strips trailing newlines from `$( ... )` output.
    Ok(String::from_utf8_lossy(&out.stdout)
        .trim_end_matches('\n')
        .to_string())
}

#[inline]
fn try_parse(content: &str, dummy_import: bool) -> Result<Context, Vec<ParseError>> {
    let mut context = Context::new();
    if dummy_import {
        for pred in DUMMY_AB_IMPORT {
            context.insert(pred.to_string(), Value::Scalar(String::new()));
        }
    }
    if std::env::var_os("CMD_SUBST").is_some() {
        parse_with_runner(content, &mut context, &mut |stages| run_sh(stages))?;
    } else {
        abbs_meta_apml::parse(content, &mut context)?;
    }
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
    let mut dump: std::collections::HashMap<String, Context> = std::collections::HashMap::new();
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

fn main() -> Result<()> {
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
