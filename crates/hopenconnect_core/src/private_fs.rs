use crate::error::{CoreError, CoreResult};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::Path;

pub(crate) fn ensure_private_dir(path: &Path) -> CoreResult<()> {
    fs::create_dir_all(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub(crate) fn write_atomic_private(path: &Path, content: &[u8]) -> CoreResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::msg("private file path has no parent"))?;
    ensure_private_dir(parent)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| CoreError::msg("private file path has no valid file name"))?;
    let temp = parent.join(format!(".{file_name}.tmp-{}", std::process::id()));

    let result = (|| -> CoreResult<()> {
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(content)?;
        file.sync_all()?;
        fs::rename(&temp, path)?;
        sync_directory(parent);
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

pub(crate) fn secure_existing_file(path: &Path) -> CoreResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::msg("private file path has no parent"))?;
    ensure_private_dir(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(())
}

fn sync_directory(path: &Path) {
    if let Ok(directory) = File::open(path) {
        let _ = directory.sync_all();
    }
}
