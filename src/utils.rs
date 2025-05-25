use std::path::Path;

pub fn make_sure_dir_exists<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
    let path = match path.as_ref().parent() {
        Some(parent) => parent,
        None => return Ok(()),
    };
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}
