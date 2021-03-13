use abbs_meta_tree::tree::Tree;
use anyhow::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    let tree_dir = std::env::var("TREE_DIR")?;
    let path = PathBuf::from(tree_dir);
    let tree = Tree::from(&path).unwrap();

    let json = serde_json::to_string(&tree)?;

    print!("{}", json);
    Ok(())
}
