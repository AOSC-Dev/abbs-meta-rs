use anyhow::{anyhow, Result};
use std::{
    fs,
    path::{self, Path, PathBuf},
};

const ALLOWED_FUNC_PREFIX: &[&str] = &[
    "map",
    "policy",
    "pool",
    "prune",
    "queue",
    "repo",
    "repodata",
    "selection",
    "solv",
    "solver",
    "testcase",
    "transaction",
    "dataiterator",
    "datamatcher",
    "stringpool",
];

fn find_system_libsolv() -> Result<PathBuf> {
    let mut conf = pkg_config::Config::new();
    let lib = conf.atleast_version("0.7").probe("libsolv")?;
    conf.atleast_version("0.7").probe("libsolvext")?;

    for inc in lib.include_paths {
        if inc.join("solv").is_dir() {
            return Ok(inc.join("solv"));
        }
    }

    Err(anyhow!("Error finding libsolv include path"))
}

fn build_libsolv() -> Result<PathBuf> {
    println!("cargo:warning=System libsolv not found. Using bundled version.");
    let p = path::PathBuf::from("./libsolv/CMakeLists.txt");
    if !p.is_file() {
        return Err(anyhow!(
            "Bundled libsolv not found, please do `git submodule update --init`."
        ));
    }
    let out = cmake::Config::new(p.parent().unwrap())
        .define("ENABLE_DEBIAN", "ON")
        .define("DEBIAN", "ON")
        .define("ENABLE_STATIC", "ON")
        .define("DISABLE_SHARED", "ON")
        .target(&std::env::var("CMAKE_TARGET").unwrap_or_else(|_| std::env::var("TARGET").unwrap()))
        .build();
    println!(
        "cargo:rustc-link-search=native={}",
        out.join("lib").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        out.join("lib64").display()
    );
    println!("cargo:rustc-link-lib=static=solv");
    println!("cargo:rustc-link-lib=static=solvext");
    println!("cargo:rustc-link-lib=z");

    Ok(out.join("include/solv"))
}

fn check_solvext_bindings(
    include_path: &Path,
    builder: bindgen::Builder,
) -> Result<bindgen::Builder> {
    let mut builder = builder;
    for inc in fs::read_dir(include_path)? {
        let inc = inc?;
        let name = inc.file_name();
        let name = name.to_string_lossy();
        // all the solvext include files are named like `repo_<format_name>.h`
        if name.starts_with("repo_") && name.ends_with(".h") {
            builder = builder.header(inc.path().to_str().unwrap());
        }
    }

    Ok(builder)
}

fn generate_bindings(include_path: &Path) -> Result<()> {
    let output = std::env::var("OUT_DIR")?;
    let generator = bindgen::Builder::default()
        .header(include_path.join("solver.h").to_str().unwrap())
        .header(include_path.join("solverdebug.h").to_str().unwrap())
        .header(include_path.join("selection.h").to_str().unwrap())
        .header(include_path.join("knownid.h").to_str().unwrap())
        .allowlist_type("(Id|solv_knownid)")
        .allowlist_var(".*")
        .allowlist_function(format!("({}).*", ALLOWED_FUNC_PREFIX.join("|")));
    check_solvext_bindings(include_path, generator)?
        .generate()
        .unwrap()
        .write_to_file(Path::new(&output).join("bindings.rs"))?;

    Ok(())
}

fn main() -> Result<()> {
    let include_path = match find_system_libsolv() {
        Ok(p) => p,
        Err(_) => build_libsolv()?,
    };
    generate_bindings(&include_path)?;

    Ok(())
}
