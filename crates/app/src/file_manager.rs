//! Open an asset's containing directory without executing the asset itself.
use anyhow::{Context, Result, ensure};
use std::path::{Path, PathBuf};

fn containing_folder(path: &Path) -> Result<PathBuf> {
    ensure!(path.is_absolute(), "The saved file path must be absolute");
    let file = path
        .canonicalize()
        .context("The file has moved or no longer exists")?;
    ensure!(file.is_file(), "The saved path is not a file");
    Ok(file
        .parent()
        .context("The file has no containing folder")?
        .to_owned())
}

pub async fn open_containing_folder(path: &Path) -> Result<()> {
    let folder = containing_folder(path)?;
    #[cfg(target_os = "windows")]
    let program = "explorer.exe";
    #[cfg(target_os = "macos")]
    let program = "open";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let program = "xdg-open";
    let mut child = tokio::process::Command::new(program)
        .arg(folder)
        .spawn()
        .context("Could not launch the file manager")?;
    let status = tokio::time::timeout(std::time::Duration::from_secs(15), child.wait()).await;
    // Some desktop launchers remain alive with the opened window. Do not kill it
    // or report a failure merely because that window stays open.
    if let Ok(status) = status {
        let status = status.context("Could not read the file manager launch result")?;
        // Explorer may return nonzero after handing off to an existing window.
        #[cfg(not(target_os = "windows"))]
        ensure!(
            status.success(),
            "The file manager could not open this folder"
        );
        #[cfg(target_os = "windows")]
        let _ = status;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_existing_absolute_files_resolve_to_folders() {
        let root = tempfile::tempdir().expect("root");
        let file = root.path().join("clip ; $(example) ' 雪.mp4");
        std::fs::write(&file, b"video").expect("file");
        assert_eq!(
            containing_folder(&file).expect("folder"),
            root.path().canonicalize().expect("root path")
        );
        assert!(containing_folder(root.path()).is_err());
        assert!(containing_folder(&root.path().join("missing.mp4")).is_err());
        assert!(containing_folder(Path::new("relative.mp4")).is_err());
        assert!(containing_folder(Path::new("https://example.com/video")).is_err());
    }
}
