//! User-scoped startup registration, changed only by an explicit UI action.
#[cfg(any(target_os = "linux", target_os = "windows", test))]
use anyhow::Context;
use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

fn registration() -> Result<PathBuf> {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    let base = directories_next::BaseDirs::new().context("user directories unavailable")?;
    #[cfg(target_os = "linux")]
    return Ok(base
        .config_dir()
        .join("autostart/obs-wow-companion.desktop"));
    #[cfg(target_os = "windows")]
    return Ok(base
        .data_dir()
        .join("Microsoft/Windows/Start Menu/Programs/Startup/OBS WoW Companion.lnk"));
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    bail!("startup registration is supported on Linux and Windows");
}

pub fn enabled() -> bool {
    registration().is_ok_and(|p| p.is_file())
}

#[cfg(any(target_os = "windows", test))]
fn windows_argument(path: &Path) -> Result<String> {
    let text = path.to_str().context("startup paths must be UTF-8")?;
    if text.contains(['"', '\r', '\n', '\0']) {
        bail!("invalid startup data path");
    }
    // The Windows argument parser pairs backslashes immediately before the closing quote.
    let trailing = text.chars().rev().take_while(|c| *c == '\\').count();
    Ok(format!("\"{text}{}\"", "\\".repeat(trailing)))
}

#[cfg(target_os = "linux")]
fn desktop_argument(path: &Path) -> Result<String> {
    let text = path.to_str().context("startup paths must be UTF-8")?;
    if text.contains(['\n', '\r', '\0']) {
        bail!("startup path contains a control character");
    }
    // Desktop Entry Exec has its own quoting rules; percent is a field-code escape.
    Ok(format!(
        "\"{}\"",
        text.replace('\\', "\\\\\\\\")
            .replace('"', "\\\\\"")
            .replace('`', "\\\\`")
            .replace('$', "\\\\$")
            .replace('%', "%%")
    ))
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
pub fn set(_enabled: bool, _data: &Path) -> Result<()> {
    bail!("startup registration is supported on Linux and Windows")
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn set(enabled: bool, data: &Path) -> Result<()> {
    let path = registration()?;
    if !enabled {
        match std::fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e.into()),
        }
    }
    let executable = std::env::current_exe()?;
    std::fs::create_dir_all(path.parent().context("startup directory")?)?;
    #[cfg(target_os = "linux")]
    {
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=OBS WoW Companion\nExec={} --data-dir {}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
            desktop_argument(&executable)?,
            desktop_argument(data)?
        );
        std::fs::write(path, entry)?;
    }
    #[cfg(target_os = "windows")]
    {
        fn literal(path: &Path) -> Result<String> {
            Ok(format!(
                "'{}'",
                path.to_str()
                    .context("startup paths must be UTF-8")?
                    .replace('\'', "''")
            ))
        }
        let args = format!("--data-dir {}", windows_argument(data)?).replace('\'', "''");
        let script = format!(
            "$ErrorActionPreference='Stop'; $shell=New-Object -ComObject WScript.Shell; $link=$shell.CreateShortcut({}); $link.TargetPath={}; $link.Arguments='{args}'; $link.Save()",
            literal(&path)?,
            literal(&executable)?
        );
        let status = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .status()?;
        if !status.success() {
            bail!("Windows startup shortcut creation failed");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_directory_arguments_preserve_trailing_backslashes() {
        assert_eq!(
            windows_argument(Path::new("C:\\")).expect("root"),
            "\"C:\\\\\""
        );
        assert_eq!(
            windows_argument(Path::new("C:\\two words\\")).expect("directory"),
            "\"C:\\two words\\\\\""
        );
        assert!(windows_argument(Path::new("bad\"argument")).is_err());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn desktop_paths_cannot_inject_arguments_or_field_codes() {
        assert_eq!(
            desktop_argument(Path::new("/tmp/a b%f")).expect("quoted"),
            "\"/tmp/a b%%f\""
        );
        assert!(desktop_argument(Path::new("/tmp/name\nExec=bad")).is_err());
    }
}
