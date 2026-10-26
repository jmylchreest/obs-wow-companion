//! Bounded discovery in common game and Wine-prefix locations.
use serde::Serialize;
use std::{
    collections::{HashSet, VecDeque},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

#[derive(Serialize)]
pub struct Candidate {
    name: &'static str,
    flavour: &'static str,
    logs: PathBuf,
    available: bool,
}
#[derive(Serialize)]
pub struct Discovery {
    installations: Vec<Candidate>,
    limited: bool,
}
pub fn discover(folder: Option<PathBuf>) -> Discovery {
    let roots = folder.map_or_else(default_roots, |path| vec![path]);
    scan(roots)
}
fn default_roots() -> Vec<PathBuf> {
    let mut roots = vec![];
    if let Ok(installations) = steamlocate::locate_all() {
        for installation in installations {
            roots.extend(crate::launcher_paths::steam_roots(installation.path()));
        }
    }
    #[cfg(target_os = "macos")]
    roots.push(PathBuf::from("/Applications/World of Warcraft"));
    if let Some(base) = directories_next::BaseDirs::new() {
        for relative in [
            "Games",
            ".wine/drive_c/Program Files",
            ".wine/drive_c/Program Files (x86)",
            ".local/share/lutris",
            ".local/share/bottles/bottles",
            ".var/app/com.usebottles.bottles/data/bottles/bottles",
            "Applications",
        ] {
            roots.push(base.home_dir().join(relative));
        }
        roots.extend(crate::launcher_paths::steam_roots(
            &base.data_dir().join("Steam"),
        ));
        let wine = base.home_dir().join(".wine");
        roots.extend(crate::launcher_paths::battle_net_roots(
            &wine.join("drive_c/ProgramData/Battle.net/Agent/product.db"),
            Some(&wine),
        ));
        for relative in [
            ".local/share/Steam",
            ".steam/steam",
            ".steam/root",
            ".var/app/com.valvesoftware.Steam/.local/share/Steam",
            "Library/Application Support/Steam",
        ] {
            roots.extend(crate::launcher_paths::steam_roots(
                &base.home_dir().join(relative),
            ));
        }
    }
    #[cfg(target_os = "macos")]
    roots.extend(crate::launcher_paths::battle_net_roots(
        Path::new("/Users/Shared/Battle.net/Agent/product.db"),
        None,
    ));
    #[cfg(target_os = "windows")]
    for variable in ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"] {
        if let Some(path) = std::env::var_os(variable) {
            roots.push(PathBuf::from(&path).join("World of Warcraft"));
            roots.extend(crate::launcher_paths::steam_roots(
                &PathBuf::from(path).join("Steam"),
            ));
        }
    }
    #[cfg(target_os = "windows")]
    if let Some(program_data) = std::env::var_os("ProgramData") {
        roots.extend(crate::launcher_paths::battle_net_roots(
            &PathBuf::from(program_data).join("Battle.net/Agent/product.db"),
            None,
        ));
    }
    roots.sort_by_key(|path| path.ends_with("compatdata"));
    roots
}
fn variant(path: &Path) -> Option<(&'static str, &'static str)> {
    match path.file_name()?.to_str()? {
        "_retail_" => Some(("Retail", "retail")),
        "_classic_" => Some(("Classic", "classic")),
        "_classic_era_" => Some(("Classic Era", "era")),
        _ => None,
    }
}
fn scan(roots: Vec<PathBuf>) -> Discovery {
    let started = Instant::now();
    let mut queue: VecDeque<_> = roots.into_iter().map(|path| (path, 0)).collect();
    let mut visited = HashSet::new();
    let mut found = vec![];
    let mut entries = 0;
    let mut limited = false;
    while let Some((path, depth)) = queue.pop_front() {
        if started.elapsed() > Duration::from_secs(2) || entries >= 20_000 {
            limited = true;
            break;
        }
        let Ok(path) = path.canonicalize() else {
            continue;
        };
        if !visited.insert(path.clone()) {
            continue;
        }
        if depth == 0 && path.join("steamapps/libraryfolders.vdf").is_file() {
            for location in crate::launcher_paths::steam_roots(&path) {
                queue.push_front((location, 0));
            }
        }
        if path.file_name().is_some_and(|name| name == "drive_c")
            && let Some(prefix) = path.parent()
        {
            for location in crate::launcher_paths::battle_net_roots(
                &path.join("ProgramData/Battle.net/Agent/product.db"),
                Some(prefix),
            ) {
                queue.push_front((location, 0));
            }
        }
        if let Some((name, flavour)) = variant(&path) {
            let logs = path.join("Logs");
            found.push(Candidate {
                name,
                flavour,
                available: std::fs::read_dir(&logs).is_ok(),
                logs,
            });
            continue;
        }
        if depth >= 10 {
            limited = true;
            continue;
        }
        let Ok(children) = std::fs::read_dir(&path) else {
            continue;
        };
        for child in children.flatten() {
            entries += 1;
            if entries >= 20_000 || started.elapsed() > Duration::from_secs(2) {
                limited = true;
                break;
            }
            if child.file_type().is_ok_and(|kind| kind.is_dir()) {
                let name = child.file_name();
                if !matches!(
                    name.to_str(),
                    Some(
                        "windows"
                            | "Windows"
                            | ".git"
                            | "node_modules"
                            | "Cache"
                            | "cache"
                            | "target"
                    )
                ) {
                    queue.push_back((child.path(), depth + 1));
                }
            }
        }
    }
    found.sort_by(|a, b| a.logs.cmp(&b.logs));
    Discovery {
        installations: found,
        limited,
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn discovers_wow_in_an_external_steam_library() {
        let root = tempfile::tempdir().expect("root");
        let steam = root.path().join("Steam");
        let external = root.path().join("Second library");
        let logs = external.join(
            "steamapps/compatdata/123/pfx/drive_c/Program Files/World of Warcraft/_retail_/Logs",
        );
        std::fs::create_dir_all(&logs).expect("logs");
        std::fs::create_dir_all(steam.join("steamapps")).expect("steam");
        let path = external.to_string_lossy().replace('\\', "\\\\");
        std::fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!(r#""libraryfolders" {{ "1" {{ "path" "{path}" }} }}"#),
        )
        .expect("metadata");
        let result = super::discover(Some(steam));
        assert_eq!(result.installations.len(), 1);
        assert_eq!(
            result.installations[0].logs,
            logs.canonicalize().expect("path")
        );
    }

    #[test]
    fn finds_multiple_clients_deduplicates_roots_and_marks_missing_logs() {
        let root = tempfile::tempdir().expect("root");
        for path in [
            "one/World of Warcraft/_retail_/Logs",
            "two/World of Warcraft/_retail_/Logs",
            "one/World of Warcraft/_classic_/Logs",
            "one/World of Warcraft/_classic_era_",
        ] {
            std::fs::create_dir_all(root.path().join(path)).expect("fixture");
        }
        let result = super::scan(vec![root.path().into(), root.path().join("one")]);
        assert_eq!(result.installations.len(), 4);
        assert_eq!(
            result.installations.iter().filter(|i| i.available).count(),
            3
        );
        assert!(!result.limited);
    }
}
