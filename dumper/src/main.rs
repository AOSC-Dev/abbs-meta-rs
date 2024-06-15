use abbs_meta_apml::{parse, ParseError};
use anyhow::Result;
use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Write},
    path::PathBuf,
};

type Context = HashMap<String, String>;
const DUMMY_AB_IMPORT: &[&str] = &["SRCDIR", "PKGDIR", "PKGVER", "PKGREL", "ARCH"];

#[inline]
fn try_parse(content: &str, dummy_import: bool) -> Result<Context, Vec<ParseError>> {
    let mut context = HashMap::new();
    if dummy_import {
        for pred in DUMMY_AB_IMPORT {
            context.insert(pred.to_string(), String::new());
        }
    }
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
