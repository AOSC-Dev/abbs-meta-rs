use anyhow::{anyhow, Result};
use std::path::{self, Path, PathBuf};

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
        .define("ENABLE_STATIC", "ON")
        .define("DISABLE_SHARED", "ON")
        .build();
    println!(
        "cargo:rustc-link-search=native={}",
        out.join("lib").display()
    );
    println!("cargo:rustc-link-lib=static=solv");
    println!("cargo:rustc-link-lib=static=solvext");

    Ok(out.join("include/solv"))
}

fn generate_bindings(include_path: &Path) -> Result<()> {
    let output = std::env::var("OUT_DIR")?;
    bindgen::Builder::default()
        .header(include_path.join("solver.h").to_str().unwrap())
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
