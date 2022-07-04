mod error;
mod fail_arch;
pub use error::{PackageError, PackageErrorType};
pub use fail_arch::FailArch;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    //section: String,
    epoch: usize,
    version: String,
    release: usize, // Revision, but in apt's dictionary
    fail_arch: Option<FailArch>,

    // HashMap<String, Vec<(String, Option<String>, Option<String>)>>  ->  HashMap<arch, Vec<(dependency, relop, version)>>
    dependencies: HashMap<String, Vec<(String, Option<String>, Option<String>)>>,
    build_dependencies: HashMap<String, Vec<(String, Option<String>, Option<String>)>>,
    package_suggests: HashMap<String, Vec<(String, Option<String>, Option<String>)>>,
    package_provides: HashMap<String, Vec<(String, Option<String>, Option<String>)>>,
    package_recommands: HashMap<String, Vec<(String, Option<String>, Option<String>)>>,
    package_replaces: HashMap<String, Vec<(String, Option<String>, Option<String>)>>,
    package_breaks: HashMap<String, Vec<(String, Option<String>, Option<String>)>>,
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
                },
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
            fail_arch: {
                if let Some(s) = context.get("FAIL_ARCH") {
                    match FailArch::from(s) {
                        Ok(res) => Some(res),
                        Err(_) => {
                            return Err(PackageError {
                                pkgname: name,
                                error: PackageErrorType::FieldSyntaxError("FAIL_ARCH".to_string()),
                            })
                        }
                    }
                } else {
                    None
                }
            },
            dependencies: get_field_with_arch_restriction("PKGDEP", context),
            build_dependencies: get_field_with_arch_restriction("BUILDDEP", context),
            package_suggests: get_field_with_arch_restriction("PKGSUG", context),
            package_provides: get_field_with_arch_restriction("PKGPROV", context),
            package_recommands: get_field_with_arch_restriction("PKGRECOM", context),
            package_replaces: get_field_with_arch_restriction("PKGREP", context),
            package_breaks: get_field_with_arch_restriction("PKGBREAK", context),
        };

        Ok(res)
    }
}

fn get_field_with_arch_restriction(
    s: &str,
    context: &HashMap<String, String>,
) -> HashMap<String, Vec<(String, Option<String>, Option<String>)>> {
    let mut dep = HashMap::new();
    dep.insert(
        "default".to_string(),
        get_items_from_bash_string(context.get(s).unwrap_or(&String::new())).iter().map(|s| split_by_relop(s)).collect(),
    );
    for (arch, deps) in get_fields_with_prefix(context, &format!("{s}__")) {
        dep.insert(arch.to_lowercase(), get_items_from_bash_string(&deps).iter().map(|s| split_by_relop(s)).collect() );
    }

    dep
}

fn split_by_relop(s: &str) -> (String, Option<String>, Option<String>) {
    let f = |relop:&str| {
        let v: Vec<&str> = s.split(relop).map(|s| s).collect();
        if v.len() == 1 {
            None
        } else {
            Some((v[0].to_string(), Some(relop.to_string()), Some(v[1].to_string())))
        }
    };

    f("<=")
        .or_else(|| f(">="))
        .or_else(|| f("=="))
        .or_else(|| f("<"))
        .or_else(|| f(">"))
        .map_or_else(|| (s.to_string(), None, None), |v| v)
}

fn get_items_from_bash_string(s: &str) -> Vec<String> {
    s.split(' ')
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<String>>()
}

/// Find all entries in the HashMap with name that has the given prefix,
///   then return a Vec with the names (prefix stripped) and the values
/// For example, PKGDEP__AMD64 with prefix="PKGDEP__" -> ("AMD64", fields) in the Vec
fn get_fields_with_prefix(h: &HashMap<String, String>, prefix: &str) -> Vec<(String, String)> {
    let mut res = Vec::new();
    for (name, value) in h {
        if name.starts_with(prefix) {
            res.push((
                name.strip_prefix(prefix).unwrap().to_string(),
                value.to_string(),
            ));
        }
    }

    res
}
