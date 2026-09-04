use std::path::Path;
/// Native file identity handles Windows case aliases, junctions and symlinks without
/// incorrectly lowercasing paths on case-sensitive filesystems. Inputs are canonical.
pub fn same_or_descendant(candidate: &Path, ancestor: &Path) -> bool {
    candidate
        .ancestors()
        .any(|path| path == ancestor || same_file::is_same_file(path, ancestor).unwrap_or(false))
}
