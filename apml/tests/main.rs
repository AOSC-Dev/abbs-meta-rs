use abbs_meta_apml::{parse, ParseError};

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
    let mut errors = 0usize;
    let mut total = 0usize;
    let walker = walkdir::WalkDir::new(std::env::var("SPEC_DIR")?).max_depth(4);
    for entry in walker.into_iter() {
        let file = entry?;
        if file.file_name() == "spec" {
            let path = PathBuf::from(file.path());
            defines.push(path);
        }
    }

    for p in defines.into_iter() {
        let mut f = File::open(&p).unwrap();
        let mut content = String::new();
        f.read_to_string(&mut content).unwrap();
        total += 1;
        if let Err(_) = try_parse(&content) {
            errors += 1;
        }
    }
    println!("Total: {}, Errors: {} ({}%)", total, errors, errors * 100 / total);

    Ok(())
}

#[test]
fn test_single_file() -> Result<()> {
    let content = "ABC='123'\nBCD=${NO}\n".to_string();
    let mut context = HashMap::new();
    let result = parse(&content, &mut context);
    if let Err(_) = result {
        return Ok(());
    }

    println!("{:#?}", context);

    Err(anyhow!("Error not caught"))
}
