mod error;
pub use error::{PackageError, PackageErrorType};

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    //section: String,
    epoch: usize,
    version: String,
    release: usize, // Revision, but in apt's dictionary
    dependencies: Vec<String>,
    arch_dependencies: HashMap<String, Vec<String>>,
    build_dependencies: Vec<String>,
    arch_build_dependencies: HashMap<String, Vec<String>>,
}

const NAME_FILED: &str = "PKGNAME";
// TODO: Add PKGSEC
const MANDATORY_FIELDS: [&str; 2] = ["PKGVER", "PKGDES"];

impl Package {
    pub fn from(context: &HashMap<String, String>) -> Result<Self, error::PackageError> {
        let name = match context.get(NAME_FILED) {
            Some(name) => name.to_string(),
            None => {
                return Err(PackageError {
                    pkgname: "Unknown".to_string(),
                    error: PackageErrorType::MissingField(NAME_FILED.to_string()),
                });
            }
        };

        for f in MANDATORY_FIELDS.iter() {
            let field_name = f.to_string();
            if !context.contains_key(&field_name) {
                return Err(PackageError {
                    pkgname: name,
                    error: PackageErrorType::MissingField(field_name),
                });
            }
        }

        // Get important fields
        let res = Package {
            name: context.get("PKGNAME").unwrap().to_string(),
            //section: context.get("PKGSEC").unwrap().to_string(),
            version: context.get("PKGVER").unwrap().to_string(),
            epoch: match context.get("PKGEPOCH") {
                Some(epoch) => match epoch.parse() {
                    Ok(epoch) => epoch,
                    Err(_e) => {
                        return Err(PackageError {
                            pkgname: name,
                            error: PackageErrorType::FieldTypeError(
                                "PKGREL".to_string(),
                                "unsigned int".to_string(),
                            ),
                        });
                    }
                }
                None => 0,
            },
            release: match context.get("PKGREL") {
                Some(rel) => match rel.parse() {
                    Ok(rel) => rel,
                    Err(_e) => {
                        return Err(PackageError {
                            pkgname: name,
                            error: PackageErrorType::FieldTypeError(
                                "PKGREL".to_string(),
                                "unsigned int".to_string(),
                            ),
                        });
                    }
                },
                None => 0,
            },
            dependencies: get_items_from_bash_string(
                context.get("PKGDEP").unwrap_or(&String::new()),
            ),
            arch_dependencies: {
                let mut arch_deps = HashMap::new();
                for (arch, deps) in get_fields_with_prefix(context, "PKGDEP__") {
                    arch_deps.insert(arch.to_lowercase(), get_items_from_bash_string(&deps));
                }
                arch_deps
            },
            build_dependencies: get_items_from_bash_string(
                context.get("BUILDDEP").unwrap_or(&String::new()),
            ),
            arch_build_dependencies: {
                let mut arch_deps = HashMap::new();
                for (arch, deps) in get_fields_with_prefix(context, "BUILDDEP__") {
                    arch_deps.insert(arch.to_lowercase(), get_items_from_bash_string(&deps));
                }
                arch_deps
            }
        };

        Ok(res)
    }
}

fn get_items_from_bash_string(s: &str) -> Vec<String> {
    s.split(' ')
        .map(|s| s.to_string())
        .filter(|s| s.len() != 0)
        .collect::<Vec<String>>()
}

/// Find all entries in the HashMap with name that has the given prefix,
///   then return a Vec with the names (prefix stripped) and the values
/// For example, PKGDEP__AMD64 with prefix="PKGDEP__" -> ("AMD64", fields) in the Vec
fn get_fields_with_prefix(h: &HashMap<String, String>, prefix: &str) -> Vec<(String, String)> {
    let mut res = Vec::new();
    for (name, value) in h {
        if name.starts_with(prefix) {
            res.push((name.strip_prefix(prefix).unwrap().to_string(), value.to_string()))
        }
    }

    res
}
