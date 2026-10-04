use std::{
    error::Error,
    fs, io,
    path::{Path, PathBuf},
};

pub fn collect_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    let metadata = fs::metadata(path)?;
    if metadata.is_file() {
        files.push(path.to_path_buf());
    } else if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_file() || file_type.is_dir() {
                collect_files(&entry.path(), files)?;
            }
        }
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("not a regular file or directory: {}", path.display()),
        )
        .into());
    }

    Ok(())
}
