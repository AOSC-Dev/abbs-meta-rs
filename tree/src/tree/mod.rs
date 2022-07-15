pub mod error;
use error::TreeError;

use super::package::Package;
use abbs_meta_apml::parse;

use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fs, path::Path};

#[derive(Debug, Serialize, Deserialize)]
pub struct Tree {
    packages: HashMap<String, Package>,
}

impl Tree {
    pub fn from(path: &Path) -> Result<Self, TreeError> {
        let walker = walkdir::WalkDir::new(path).max_depth(4);
        let mut pkg_dirs = Vec::new();
        for entry in walker.into_iter() {
            let file = entry?;
            if file.file_name() == "defines" {
                let pkg_dir = file
                    .path()
                    .parent()
                    .ok_or_else(|| TreeError::FsError(format!(
                    "The directory of defines file {} is root.",
                    file.path().display()
                )))?
                    .parent()
                    .ok_or_else(|| TreeError::FsError(format!(
                    "The parent directory of defines file {} is root.",
                    file.path().display()
                )))?;
                let spec_path = pkg_dir.join("spec");
                if !spec_path.is_file() {
                    return Err(TreeError::FsError(format!(
                        "spec file not found at {} for {}",
                        spec_path.display(),
                        file.path().display()
                    )));
                }
                pkg_dirs.push((spec_path, file.path().to_path_buf()));
            }
        }

        let mut res = Tree {
            packages: HashMap::new(),
        };
        for (spec_path, defines_path) in pkg_dirs {
            let spec = fs::read_to_string(&spec_path)?;
            let defines = fs::read_to_string(&defines_path)?;
            let mut context = HashMap::new();

            // First parse spec
            if let Err(e) = parse(&spec, &mut context) {
                let e: Vec<String> = e.iter().map(|e| e.to_string()).collect();
                eprintln!("Failed to parse {}: {:?}, skipping.", spec_path.display(), e);
                continue;
            }
            // Modify context so that defines can understand
            spec_decorator(&mut context);
            // Then parse defines
            if let Err(e) = parse(&defines, &mut context) {
                let e: Vec<String> = e.iter().map(|e| e.to_string()).collect();
                eprintln!(
                    "Failed to parse {}: {:?}, skipping.",
                    defines_path.display(),
                    e
                );
                continue;
            }
            // Parse the result into a Package
            let pkg = Package::from(&context,&spec_path)?;
            if res.packages.contains_key(&pkg.name) {
                eprintln!(
                    "Duplicate package name {} found at {}, ignoring.",
                    &pkg.name,
                    defines_path.display()
                );
            } else {
                res.packages.insert(pkg.name.clone(), pkg);
            }
        }

        Ok(res)
    }
}

fn spec_decorator(c: &mut HashMap<String, String>) {
    if let Some(ver) = c.remove("VER") {
        c.insert("PKGVER".to_string(), ver);
    }

    if let Some(rel) = c.remove("REL") {
        c.insert("PKGREL".to_string(), rel);
    }
}
