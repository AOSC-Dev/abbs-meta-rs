mod error;
use error::PackageError;

use std::collections::HashMap;

pub struct Package {
    name: String,
    section: String,
    version: String,
    release: Option<usize>, // Revision, but in apt's dictionary
    dependencies: Vec<String>,
    build_dependencies: Vec<String>,
}

const MANDATORY_FIELDS: [&str; 4] = ["PKGNAME", "PKGSEC", "PKGVER", "PKGDES"];

impl Package {
    pub fn from(context: &HashMap<String, String>) -> Result<Self, error::PackageError> {
        for f in MANDATORY_FIELDS.iter() {
            let field_name = f.to_string();
            if !context.contains_key(&field_name) {
                return Err(PackageError::MissingField(field_name));
            }
        }

        // Get important fields
        let res = Package {
            name: context.get("PKGNAME").unwrap().to_string(),
            section: context.get("PKGSEC").unwrap().to_string(),
            version: context.get("PKGVER").unwrap().to_string(),
            release: match context.get("PKGREL") {
                Some(rel) => match rel.parse() {
                    Ok(rel) => Some(rel),
                    Err(_e) => {
                        return Err(PackageError::FieldTypeError(
                            "PKGREL".to_string(),
                            "unsigned int".to_string(),
                        ))
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
    s.split(' ').map(|s| s.to_string()).collect::<Vec<String>>()
}
