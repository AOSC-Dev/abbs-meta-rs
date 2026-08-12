//! `abbs-lint` — lint and autofix AOSC OS `spec` / `defines` files.
//!
//! The walk replicates the abbs-meta-collector flow: for each package, the
//! `spec` is parsed first (its variables become the initial context), then
//! each `defines` file is parsed and linted against that context.
//!
//! Usage:
//!
//! ```text
//! abbs-lint [check] [--tree DIR]   # report findings (default)
//! abbs-lint fix [--tree DIR]       # apply autofixes to files
//! ```
//!
//! `--tree DIR` may be replaced by the `ABBS_DIR` environment variable.

use abbs_meta_apml::{parse, Context, DiagnosticInfo, DiagnosticSpan, Lint, LintFix, LintSeverity};
use annotate_snippets::{AnnotationKind, Level, Renderer, Snippet};
use anyhow::{bail, Context as AnyhowContext, Result};
use clap::{Parser, Subcommand};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Lint and autofix AOSC OS `spec` / `defines` files.
#[derive(Parser)]
#[command(name = "abbs-lint", version, about)]
struct Cli {
    /// Tree root directory (or the ABBS_DIR environment variable).
    #[arg(long, global = true, env = "ABBS_DIR")]
    tree: Option<PathBuf>,

    /// Number of parallel workers (default: number of CPUs).
    #[arg(long, global = true, default_value_t = default_jobs())]
    jobs: usize,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Report lint findings (default).
    Check,
    /// Apply autofixes to files.
    Fix,
}

/// A finding ready to be reported or applied.
struct Finding {
    path: PathBuf,
    severity: &'static str, // "error" | "warning"
    rule: String,
    span: DiagnosticSpan,
    message: String,
    fix: Option<LintFix>,
}

/// Default parallelism: the number of CPUs available to the process.
fn default_jobs() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cmd = cli.command.unwrap_or(Command::Check);

    rayon::ThreadPoolBuilder::new()
        .num_threads(cli.jobs)
        .build_global()
        .expect("failed to build rayon thread pool");

    let spec_dir = cli
        .tree
        .context("missing tree: pass --tree DIR or set ABBS_DIR")?;

    let findings = scan_tree(&spec_dir)?;
    let fixable = findings.iter().filter(|f| f.fix.is_some()).count();

    match cmd {
        Command::Check => {
            let mut errors = 0;
            let mut warnings = 0;
            for f in &findings {
                match f.severity {
                    "error" => errors += 1,
                    _ => warnings += 1,
                }
                print_finding(f);
            }
            println!(
                "Total: {} findings ({} errors, {} warnings; {} fixable).",
                findings.len(),
                errors,
                warnings,
                fixable
            );
        }
        Command::Fix => {
            apply_fixes(&findings)?;
            println!(
                "Fixed {} finding(s) across {} file(s) ({} findings remain).",
                fixable,
                findings
                    .iter()
                    .filter(|f| f.fix.is_some())
                    .map(|f| &f.path)
                    .collect::<std::collections::HashSet<_>>()
                    .len(),
                findings.len() - fixable,
            );
        }
    }
    Ok(())
}

/// Render one finding with an annotated source snippet (compiler style),
/// plus the suggested fix as a help line.
fn print_finding(f: &Finding) {
    let source = match fs::read_to_string(&f.path) {
        Ok(s) => s,
        // No source to render; fall back to a compact one-liner.
        Err(_) => {
            println!(
                "{}:{}:{}: [{} {}] {}",
                f.path.display(),
                f.span.line,
                f.span.col,
                f.severity,
                f.rule,
                f.message
            );
            return;
        }
    };
    let range = f.span.highlight_range(&source);
    let level = match f.severity {
        "error" => Level::ERROR,
        _ => Level::WARNING,
    };
    let path_str = f.path.display().to_string();
    let title = format!("{}[{}]", f.severity, f.rule);
    let report = &[level.primary_title(&title).element(
        Snippet::source(&source)
            .line_start(1)
            .path(path_str.as_str())
            .fold(true)
            .annotation(AnnotationKind::Primary.span(range).label(&f.message)),
    )];
    print!("{}", Renderer::styled().render(report));
    println!();
    if let Some(fix) = &f.fix {
        let end = fix.end.min(source.len());
        let old = &source[fix.start..end];
        println!("  = help: replace `{old}` with `{}`", fix.replacement);
        println!();
    }
}

/// Walk the tree the way the collector does: per package, parse the `spec`
/// once, then parse + lint every `defines` file against that context.
///
/// Packages are scanned in parallel (each is independent); findings are
/// sorted so the output is deterministic regardless of thread count.
fn scan_tree(root: &Path) -> Result<Vec<Finding>> {
    let mut defines_by_pkg: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();

    for entry in WalkDir::new(root).max_depth(4) {
        let entry = entry?;
        if entry.file_name() == "defines" {
            let def = entry.path().to_path_buf();
            // <pkg>/<subdir>/defines -> package dir
            if let (Some(_sub), Some(pkg)) = (def.parent(), def.parent().and_then(Path::parent)) {
                defines_by_pkg.entry(pkg.to_path_buf()).or_default().push(def);
            }
        }
    }

    let mut findings: Vec<Finding> = defines_by_pkg
        .into_par_iter()
        .map(|(pkg_dir, defs)| scan_package(&pkg_dir, &defs))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();

    findings.sort_by(|a, b| {
        (&a.path, a.span.line, a.span.col).cmp(&(&b.path, b.span.line, b.span.col))
    });
    Ok(findings)
}

/// Scan one package: the `spec` seeds the context, then every `defines` is
/// parsed and linted against it. Independent of every other package, so it
/// can run on any worker thread.
fn scan_package(pkg_dir: &Path, defs: &[PathBuf]) -> Result<Vec<Finding>> {
    let spec_path = pkg_dir.join("spec");
    let mut context = Context::new();
    let mut findings = Vec::new();

    // Parse the spec once: it seeds the context and is reported once.
    if spec_path.exists() {
        let source = fs::read_to_string(&spec_path)?;
        let result = parse(&source, &mut context);
        collect_parse_diagnostics(&spec_path, &result, &mut findings);
        findings.extend(
            abbs_meta_apml::lint(&source, &Context::new())
                .into_iter()
                .map(|l| to_finding(&spec_path, l)),
        );
        spec_decorator(&mut context);
    }

    for def in defs {
        let source = fs::read_to_string(def)?;
        let mut file_findings = Vec::new();

        // Collector semantics: parse evaluates into the shared context.
        let result = parse(&source, &mut context);
        collect_parse_diagnostics(def, &result, &mut file_findings);

        // Lint rules against the spec-seeded context.
        file_findings.extend(
            abbs_meta_apml::lint(&source, &context)
                .into_iter()
                .map(|l| to_finding(def, l)),
        );

        findings.extend(file_findings);
    }
    Ok(findings)
}

/// Turn parse() diagnostics into findings. The `undefined-variable` warning
/// is skipped here — the lint rules report it with richer information.
fn collect_parse_diagnostics(
    path: &Path,
    result: &abbs_meta_apml::ParseResult,
    out: &mut Vec<Finding>,
) {
    for d in &result.errors {
        out.push(Finding {
            path: path.to_path_buf(),
            severity: "error",
            rule: "parse".to_string(),
            span: d.span,
            message: match &d.info {
                DiagnosticInfo::Error(e) => e.to_string(),
                DiagnosticInfo::Warning(_) => String::new(),
            },
            fix: None,
        });
    }
    for d in &result.warnings {
        let msg = match &d.info {
            DiagnosticInfo::Warning(m) => m.clone(),
            DiagnosticInfo::Error(_) => continue,
        };
        if msg.contains("never defined") {
            // Covered by the lint `undefined-variable` rule.
            continue;
        }
        out.push(Finding {
            path: path.to_path_buf(),
            severity: "warning",
            rule: "parse".to_string(),
            span: d.span,
            message: msg,
            fix: None,
        });
    }
}

fn to_finding(path: &Path, l: Lint) -> Finding {
    Finding {
        path: path.to_path_buf(),
        severity: match l.severity {
            LintSeverity::Error => "error",
            LintSeverity::Warning => "warning",
        },
        rule: l.rule.to_string(),
        span: l.span,
        message: l.message,
        fix: l.fix,
    }
}

fn spec_decorator(c: &mut Context) {
    if let Some(ver) = c.remove("VER") {
        c.insert("PKGVER".to_string(), ver);
    }
    if let Some(rel) = c.remove("REL") {
        c.insert("PKGREL".to_string(), rel);
    }
}

/// Apply every fix, grouped by file. Files are fixed in parallel; the edits
/// within a file are applied from the end backwards so earlier byte offsets
/// stay valid.
fn apply_fixes(findings: &[Finding]) -> Result<()> {
    let mut by_file: HashMap<&Path, Vec<&LintFix>> = HashMap::new();
    for f in findings {
        if let Some(fix) = &f.fix {
            by_file.entry(f.path.as_path()).or_default().push(fix);
        }
    }

    by_file
        .into_par_iter()
        .try_for_each(|(path, fixes)| apply_fixes_to_file(path, &fixes))
}

/// Apply every fix for one file. Edits are applied from the end of the file
/// backwards so earlier byte offsets stay valid.
fn apply_fixes_to_file(path: &Path, fixes: &[&LintFix]) -> Result<()> {
    let mut fixes = fixes.to_vec();
    fixes.sort_by_key(|f| std::cmp::Reverse(f.start));

    // Reject overlapping fixes before touching the file.
    for w in fixes.windows(2) {
        if w[0].start < w[1].end {
            bail!(
                "overlapping fixes in {} ({}..{} and {}..{}) — aborting",
                path.display(),
                w[1].start,
                w[1].end,
                w[0].start,
                w[0].end
            );
        }
    }

    let mut source = fs::read_to_string(path)?;
    let orig_len = source.len();
    for fix in fixes {
        if fix.end > source.len() {
            bail!("fix out of range in {}: {}..{}", path.display(), fix.start, fix.end);
        }
        source.replace_range(fix.start..fix.end, &fix.replacement);
    }
    if source.len() != orig_len || source != fs::read_to_string(path)? {
        fs::write(path, &source)?;
        println!("fixed {}", path.display());
    }

    Ok(())
}
