use abbs_meta::{parse, ParseError};

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::io::Read;
use std::{fs::File, path::PathBuf};

fn try_parse(content: &str) -> Result<(), ParseError> {
    let mut context = HashMap::new();
    parse(content, &mut context)?;

    Ok(())
}

#[test]
fn parse_whole_tree() -> Result<()> {
    // Code for speed testing
    let mut defines = Vec::new();
    let walker = walkdir::WalkDir::new(std::env::var("SPEC_DIR")?).max_depth(4);
    for entry in walker.into_iter() {
        let file = entry?;
        if file.file_name() == "defines" {
            let path = PathBuf::from(file.path());
            defines.push(path);
        }
    }

    for p in defines.into_iter() {
        let mut f = File::open(&p).unwrap();
        let mut content = String::new();
        f.read_to_string(&mut content).unwrap();
        if let Err(e) = try_parse(&content) {
            eprintln!(
                "{}",
                e.pretty_print(&content, &p.as_path().to_string_lossy())
            );
        }
    }

    Ok(())
}

#[test]
fn test_single_file() -> Result<()> {
    let content = "ABC='123'\nBCD=${NO}\n".to_string();
    let mut context = HashMap::new();
    let result = parse(&content, &mut context);
    if let Err(e) = result {
        println!("{}", e.pretty_print(&content, "test.sh"));
        return Ok(());
    }

    println!("{:#?}", context);

    Err(anyhow!("Error not caught"))
}
