use abbs_meta_apml::{parse, ParseError};

use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::io::Read;
use std::{fs::File, path::PathBuf};

fn try_parse(content: &str) -> Result<(), Vec<ParseError>> {
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
        if try_parse(&content).is_err() {
            errors += 1;
        }
    }
    println!(
        "Total: {}, Errors: {} ({}%)",
        total,
        errors,
        errors * 100 / total
    );

    Ok(())
}

#[test]
fn test_simple() -> Result<()> {
    let content = "ABC='123'\nBCD=${ABC};A__C=${BCD/3/1}\n".to_string();
    let mut context = HashMap::new();
    parse(&content, &mut context).unwrap();
    assert_eq!(context.get("ABC"), Some(&"123".to_string()));
    assert_eq!(context.get("BCD"), Some(&"123".to_string()));
    assert_eq!(context.get("A__C"), Some(&"121".to_string()));

    Ok(())
}

#[test]
fn test_single_file_failure() -> Result<()> {
    let content = "ABC='123'\nBCD=${NO}\n".to_string();
    let mut context = HashMap::new();
    let result = parse(&content, &mut context);
    if result.is_err() {
        return Ok(());
    }

    println!("{:#?}", context);

    Err(anyhow!("Error not caught"))
}
