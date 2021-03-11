mod error;
pub use error::{PackageError, PackageErrorType};

use std::collections::HashMap;

#[derive(Debug)]
pub struct Package {
    pub name: String,
    //section: String,
    version: String,
    release: Option<usize>, // Revision, but in apt's dictionary
    dependencies: Vec<String>,
    build_dependencies: Vec<String>,
}

const NAME_FILED: &str = "PKGNAME";
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
            release: match context.get("PKGREL") {
                Some(rel) => match rel.parse() {
                    Ok(rel) => Some(rel),
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
                None => None,
            },
            dependencies: get_items_from_bash_string(
                context.get("PKGDEP").unwrap_or(&String::new()),
            ),
            build_dependencies: get_items_from_bash_string(
                context.get("BUILDDEP").unwrap_or(&String::new()),
            ),
        };

        Ok(res)
    }
}

fn get_items_from_bash_string(s: &str) -> Vec<String> {
    s.split(' ').map(|s| s.to_string()).filter(|s| s.len() != 0).collect::<Vec<String>>()
}
