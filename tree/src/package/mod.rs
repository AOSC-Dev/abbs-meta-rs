mod error;
mod fail_arch;
mod pkgsec;
pub use error::{PackageError, PackageErrorType};
pub use fail_arch::FailArch;

use pkgsec::check_pkgsec;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

/// HashMap<String, Vec<(String, Option<String>, Option<String>)>>  ->  HashMap<arch, Vec<(dependency, relop, version)>>
type PackageDepDependencies = HashMap<String, Vec<(String, Option<String>, Option<String>)>>;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub epoch: usize,
    pub version: String,
    pub category: String,
    pub section: String,
    pub directory: String,
    pub pkg_section: String,
    pub release: String, // Revision, but in apt's dictionary
    pub fail_arch: Option<FailArch>,
    pub description: String,
    pub spec_path: String,

    pub dependencies: PackageDepDependencies,
    pub build_dependencies: PackageDepDependencies,
    pub package_suggests: PackageDepDependencies,
    pub package_provides: PackageDepDependencies,
    pub package_recommands: PackageDepDependencies,
    pub package_replaces: PackageDepDependencies,
    pub package_breaks: PackageDepDependencies,
    pub package_configs: PackageDepDependencies,
}

const NAME_FILED: &str = "PKGNAME";
// TODO: Add PKGSEC
const MANDATORY_FIELDS: [&str; 3] = ["PKGVER", "PKGDES", "PKGSEC"];
const ABBS_CATEGORIES: [&str; 6] = ["app-", "core-", "desktop-", "lang-", "meta-", "runtime-"];

#[derive(Debug, Deserialize)]
pub struct SourceInfo {
    #[serde(rename = "PKGNAME")]
    pub pkg_name: String,
    #[serde(rename = "PKGDES")]
    pub desc: String,
    #[serde(rename = "PKGVER")]
    pub version: String,
    #[serde(rename = "PKGREL")]
    pub release: String,
    #[serde(rename = "PKGDEP")]
    pub deps: Option<String>,
    #[serde(rename = "BUILDDEP")]
    pub build_deps: Option<String>,
    #[serde(rename = "PKGBREAK")]
    pub breaks: Option<String>,
    #[serde(rename = "PKGSUG")]
    pub package_suggests: Option<String>,
    #[serde(rename = "PKGPROV")]
    pub package_provides: Option<String>,
    #[serde(rename = "PKGRECOM")]
    pub package_recommands: Option<String>,
    #[serde(rename = "PKGREP")]
    pub package_replaces: Option<String>,
    #[serde(rename = "PKGCONFL")]
    pub package_configs: Option<String>,
    #[serde(rename = "FAIL_ARCH")]
    pub fail_arch: Option<String>,
    #[serde(rename = "PKGEPOCH")]
    pub epoch: Option<usize>,
    #[serde(rename = "PKGSEC")]
    pub section: String,
}

impl Package {
    pub fn from_srcinfo(json: &SourceInfo, spec_path: &Path) -> Result<Self, error::PackageError> {
        let name = json.pkg_name.clone();

        // extract /tmp/aosc-os-abbs/extra-admin/packagekit/spec -> category: extra  section: admin
        let spec_path_str = spec_path.to_str().ok_or(PackageError {
            pkgname: name.clone(),
            error: PackageErrorType::FieldSyntaxError("CATEGORY".to_string()),
        })?;

        let mut category = String::new();
        let mut section = String::new();
        for abbs_category in ABBS_CATEGORIES {
            if let Some(pos1) = spec_path_str.find(abbs_category) {
                if let Some(pos2) = spec_path_str[pos1..].find('/') {
                    let category_and_section = &spec_path_str[pos1..][..pos2];
                    section = category_and_section[abbs_category.len()..].to_string();
                    category = abbs_category[..abbs_category.len() - 1].to_string();
                }
            }
        }

        let res = Package {
            name: name.clone(),
            epoch: json.epoch.unwrap_or(0),
            version: json.version.clone(),
            section,
            category,
            directory: {
                let err = PackageError {
                    pkgname: name.clone(),
                    error: PackageErrorType::FieldSyntaxError("DIRECTORY".to_string()),
                };
                let mut spec_path = spec_path.to_path_buf();
                spec_path.pop().then(|| ()).ok_or_else(|| err.clone())?;
                let directory = spec_path
                    .file_name()
                    .ok_or_else(|| err.clone())?
                    .to_str()
                    .ok_or(err)?;
                directory.to_string()
            },
            pkg_section: json.section.clone(),
            release: json.release.clone(),
            fail_arch: json
                .fail_arch
                .as_deref()
                .and_then(|x| FailArch::from(x).ok()),
            dependencies: get_field_with_arch_restriction_srcinfo(
                &json.deps.clone().unwrap_or_else(|| String::new()),
            ),
            description: json.desc.clone(),
            spec_path: spec_path.to_string_lossy().to_string(),
            build_dependencies: get_field_with_arch_restriction_srcinfo(
                &json.build_deps.clone().unwrap_or_else(|| String::new()),
            ),
            package_suggests: get_field_with_arch_restriction_srcinfo(
                &json
                    .package_suggests
                    .clone()
                    .unwrap_or_else(|| String::new()),
            ),
            package_provides: get_field_with_arch_restriction_srcinfo(
                &json
                    .package_provides
                    .clone()
                    .unwrap_or_else(|| String::new()),
            ),
            package_breaks: get_field_with_arch_restriction_srcinfo(
                &json.breaks.clone().unwrap_or_else(|| String::new()),
            ),
            package_recommands: get_field_with_arch_restriction_srcinfo(
                &json
                    .package_recommands
                    .clone()
                    .unwrap_or_else(|| String::new()),
            ),
            package_replaces: get_field_with_arch_restriction_srcinfo(
                &json
                    .package_replaces
                    .clone()
                    .unwrap_or_else(|| String::new()),
            ),
            package_configs: get_field_with_arch_restriction_srcinfo(
                &json
                    .package_configs
                    .clone()
                    .unwrap_or_else(|| String::new()),
            ),
        };

        Ok(res)
    }

    pub fn from(
        context: &HashMap<String, String>,
        spec_path: &Path,
    ) -> Result<Self, error::PackageError> {
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

        // extract /tmp/aosc-os-abbs/extra-admin/packagekit/spec -> category: extra  section: admin
        let spec_path_str = spec_path.to_str().ok_or(PackageError {
            pkgname: name.clone(),
            error: PackageErrorType::FieldSyntaxError("CATEGORY".to_string()),
        })?;
        let mut category = String::new();
        let mut section = String::new();
        for abbs_category in ABBS_CATEGORIES {
            if let Some(pos1) = spec_path_str.find(abbs_category) {
                if let Some(pos2) = spec_path_str[pos1..].find('/') {
                    let category_and_section = &spec_path_str[pos1..][..pos2];
                    section = category_and_section[abbs_category.len()..].to_string();
                    category = abbs_category[..abbs_category.len() - 1].to_string();
                }
            }
        }

        let pkg_section = check_pkgsec(
            &name.as_str(),
            context.get("PKGSEC").unwrap_or(&"".to_string()).to_owned(),
        )?;

        // Get important fields
        let res = Package {
            name: context.get("PKGNAME").unwrap().to_string(),
            version: context.get("PKGVER").unwrap().to_string(),
            epoch: match context.get("PKGEPOCH") {
                Some(epoch) => match epoch.parse() {
                    Ok(epoch) => epoch,
                    Err(_e) => {
                        return Err(PackageError {
                            pkgname: name,
                            error: PackageErrorType::FieldTypeError(
                                "PKGEPOCH".to_string(),
                                "unsigned int".to_string(),
                            ),
                        });
                    }
                },
                None => 0,
            },
            release: match context.get("PKGREL") {
                Some(rel) => rel.to_string(),
                None => "0".to_string(),
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
            package_configs: get_field_with_arch_restriction("PKGCONFL", context),
            pkg_section,
            category,
            section,
            directory: {
                let err = PackageError {
                    pkgname: name.clone(),
                    error: PackageErrorType::FieldSyntaxError("DIRECTORY".to_string()),
                };
                let mut spec_path = spec_path.to_path_buf();
                spec_path.pop().then(|| ()).ok_or_else(|| err.clone())?;
                let directory = spec_path
                    .file_name()
                    .ok_or_else(|| err.clone())?
                    .to_str()
                    .ok_or(err)?;
                directory.to_string()
            },
            description: context.get("PKGDES").expect("").to_string(),
            spec_path: spec_path
                .to_str()
                .ok_or(PackageError {
                    pkgname: name,
                    error: PackageErrorType::FieldSyntaxError("SPEC_PATH".to_string()),
                })?
                .to_string(),
        };

        Ok(res)
    }
}

fn get_field_with_arch_restriction(
    s: &str,
    context: &HashMap<String, String>,
) -> PackageDepDependencies {
    let mut dep = HashMap::new();
    dep.insert(
        "default".to_string(),
        get_items_from_bash_string(context.get(s).unwrap_or(&String::new()))
            .iter()
            .map(|s| split_by_relop(s))
            .collect(),
    );
    for (arch, deps) in get_fields_with_prefix(context, &format!("{s}__")) {
        dep.insert(
            arch.to_lowercase(),
            get_items_from_bash_string(&deps)
                .iter()
                .map(|s| split_by_relop(s))
                .collect(),
        );
    }

    dep
}

fn get_field_with_arch_restriction_srcinfo(deps: &str) -> PackageDepDependencies {
    let mut dep = HashMap::new();
    dep.insert(
        "default".to_string(),
        get_items_from_bash_string(deps)
            .iter()
            .map(|s| split_by_relop(s))
            .collect(),
    );
    // for (arch, deps) in get_fields_with_prefix(context, &format!("{s}__")) {
    //     dep.insert(
    //         arch.to_lowercase(),
    //         get_items_from_bash_string(&deps)
    //             .iter()
    //             .map(|s| split_by_relop(s))
    //             .collect(),
    //     );
    // }

    dep
}

// autogen<=5.18.12-1 -> (autogen, Some(<=), Some(5.18.12-1))
fn split_by_relop(s: &str) -> (String, Option<String>, Option<String>) {
    let f = |relop: &str| {
        let v: Vec<&str> = s.split(relop).collect();
        if v.len() == 1 {
            None
        } else {
            Some((
                v[0].to_string(),
                Some(relop.to_string()),
                Some(v[1].to_string()),
            ))
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

#[test]
fn test_parse_source_info() {
    let s = r#"{"ABAPMS":"","ABBUILD":"amd64","ABMK":null,"ABMPM":"dpkg","ABPATCHLAX":"no","ABPY2SHORTVER":"27","ABPY2VER":"2.7","ABPY3SHORTVER":"310","ABPY3VER":"3.10","ABSPLITDBG":"yes","ABSTRIP":"1","ABTHREADS":"17","AB_FLAGS_EXC":"1","AB_FLAGS_FTF":"1","AB_FLAGS_NOW":"1","AB_FLAGS_O3":"0","AB_FLAGS_OS":"0","AB_FLAGS_PIC":"1","AB_FLAGS_PIE":"1","AB_FLAGS_RRO":"1","AB_FLAGS_SCP":"1","AB_FLAGS_SPECS":"1","AB_FLAGS_SSP":"1","AB_SAN_ADD":"0","AB_SAN_LEK":"0","AB_SAN_THR":"0","AUTOTOOLS_AFTER":null,"AUTOTOOLS_DEF":["--prefix=/usr","--sysconfdir=/etc","--localstatedir=/var","--libdir=/usr/lib","--bindir=/usr/bin","--sbindir=/usr/bin","--mandir=/usr/share/man"],"BUILDDEP":"pybind11 setuptools-python3 pip","CARGO_AFTER":null,"CMAKE_AFTER":null,"CMAKE_DEF":["-DCMAKE_INSTALL_PREFIX=/usr","-DCMAKE_BUILD_TYPE=RelWithDebInfo","-DCMAKE_INSTALL_LIBDIR=lib","-DLIB_INSTALL_DIR=lib","-DSYSCONF_INSTALL_DIR=/etc","-DCMAKE_INSTALL_SBINDIR=/usr/bin","-DCMAKE_SKIP_INSTALL_RPATH=ON","-DCMAKE_VERBOSE_MAKEFILE=ON"],"DUNE_BUILD_AFTER":null,"DUNE_INSTALL_AFTER":null,"DUNE_PACKAGES":null,"INSTALL_AFTER":null,"MAKE_AFTER":null,"MAKE_INSTALL_DEF":["PREFIX=/usr","BINDIR=/usr/bin","SBINDIR=/usr/bin","LIBDIR=/usr/lib","INCDIR=/usr/include","MANDIR=/usr/share/man","prefix=/usr","bindir=/usr/bin","sbindir=/usr/bin","libdir=/usr/lib","incdir=/usr/include","mandir=/usr/share/man"],"MESON_AFTER":null,"MESON_DEF":["--prefix=/usr","--sbindir=/usr/bin"],"MTER":"Bot <null@aosc.io>","NOLTO":"no","NOPARALLEL":null,"NOSTATIC":"yes","PKGBREAK":null,"PKGCONFL":null,"PKGDEP":"autobuild4 file libarchive pexpect git mercurial wget pyparsing         pycryptodome ptyprocess","PKGDES":"Autobuild CI Build System","PKGEPOCH_SPIRAL":null,"PKGESS":null,"PKGNAME":"acbs","PKGPRDEP":null,"PKGPROV":null,"PKGRECOM":null,"PKGREL":"0","PKGREP":null,"PKGSEC":"devel","PKGSUG":null,"PKGVER":"20240810","QTPROJ_AFTER":null,"QTPROJ_DEF":["PREFIX=/usr","LIBDIR=/usr/lib","CONFIG+=force_debug_info"],"QT_SELECT":null,"RECONF":"yes","SYMTAB":null,"USECLANG":"no","VERSTAMP":"~pre20240810T083023Z-dirty","WAF_AFTER":null,"WAF_DEF":["--prefix=/usr","--configdir=/etc","--libdir=/usr/lib"]}"#;
    let info: SourceInfo = serde_json::from_str(&s).unwrap();
    let pkg = Package::from_srcinfo(&info, Path::new("/tree/app-admin/acbs")).unwrap();
    assert_eq!(pkg.name, "acbs");
    assert_eq!(pkg.category, "app");
    assert_eq!(pkg.directory, "app-admin");
    dbg!(pkg);
}
