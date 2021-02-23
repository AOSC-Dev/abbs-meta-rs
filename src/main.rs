mod apf;

use anyhow::Result;
use std::fs::File;
use std::io::Read;
use std::collections::HashMap;
//use rayon::prelude::*;

const SPEC_DIR: &str = "";
const TEST_PATH: &str = "";

fn main() -> Result<()> {
    /* Code for speed testing
    let mut defines = Vec::new();
    let walker = walkdir::WalkDir::new(SPEC_DIR).max_depth(4);
    for entry in walker.into_iter() {
        let file = entry?;
        if file.file_name() == "defines" {
            let path = PathBuf::from(file.path());
            defines.push(path);
        }
    }

    for p in defines.into_iter() {
        eprintln!("Parsing {:?}...", p.as_os_str());
        let mut f = File::open(&p).unwrap();
        let mut content = String::new();
        f.read_to_string(&mut content).unwrap();
        if try_parse(&content).is_err() {
            eprintln!("Parser failure: {:?}", &p);
        }
        break;
    }
    */
    let mut f = File::open(TEST_PATH)?;
    let mut content = String::new();
    let mut context =  HashMap::new();
    f.read_to_string(&mut content)?;
    apf::try_parse(&content, &mut context)?;

    println!("{:#?}", context);
    Ok(())
}
