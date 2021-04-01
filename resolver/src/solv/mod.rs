mod ffi;
use std::path::Path;

use anyhow::Result;
pub use ffi::{Pool, Queue, Repo, Solver, Transaction, SOLVER_FLAG_BEST_OBEY_POLICY};

/// Simulate the apt dependency resolution
pub fn simulate_install(pool: &mut Pool, name: &str) -> Result<()> {
    let mut q = Queue::new();
    q = pool.match_package(name, q)?;
    q.mark_all_for_install();
    let mut solver = Solver::new(pool);
    solver.set_flag(SOLVER_FLAG_BEST_OBEY_POLICY, 1)?;
    solver.solve(&mut q)?;

    Ok(())
}

/// Populate the packages pool with metadata
pub fn populate_pool(pool: &mut Pool, paths: &[&Path]) -> Result<()> {
    let mut repo = Repo::new(pool, "stable")?;
    for path in paths {
        repo.add_debpackages(path)?;
    }
    pool.createwhatprovides();

    Ok(())
}
