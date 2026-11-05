use std::path::{Path, PathBuf};

pub fn class_files(directory: &Path) -> Vec<PathBuf> {
    let mut pending = vec![directory.to_owned()];
    let mut files = Vec::new();
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                pending.push(entry.path());
            } else if entry
                .path()
                .extension()
                .is_some_and(|value| value == "class")
            {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    files
}
