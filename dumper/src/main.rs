use abbs_meta_apml;
use abbs_meta_apml::{parse, ParseError};
use anyhow::Result;
use serde::Serialize;
use std::{collections::HashMap, fs::File, io::{Read, Write}, path::PathBuf};

type Context = HashMap<String, String>;

fn try_parse(content: &str) -> Result<Context, ParseError> {
    let mut context = HashMap::new();
    parse(content, &mut context)?;

    Ok(context)
}

fn dump_whole_tree(is_spec: bool) -> Result<String> {
    // Code for speed testing
    let spec_dir = std::env::var("SPEC_DIR")?;
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
        if let Ok(context) = try_parse(&content) {
            let name = p.strip_prefix(&spec_dir)?;
            dump.insert(name.to_string_lossy().to_string(), context);
        } else {
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
    let dump = dump_whole_tree(true)?;
    let mut f = File::create("/tmp/all_vars_rs.json")?;
    f.write_all(dump.as_bytes())?;
    println!("[defines] Collecting variables ...");
    let dump = dump_whole_tree(false)?;
    let mut f = File::create("/tmp/all_vars_def_rs.json")?;
    f.write_all(dump.as_bytes())?;

    Ok(())
}
