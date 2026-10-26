//! Read-only, size-limited launcher hints. Every returned path is checked by discovery.
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

fn read_small(path: &Path) -> Option<Vec<u8>> {
    let mut bytes = vec![];
    File::open(path)
        .ok()?
        .take(1_048_577)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() <= 1_048_576).then_some(bytes)
}
pub fn steam_roots(steam: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![steam.to_owned()];
    // Reject unusually large metadata before handing it to the upstream parser.
    if std::fs::metadata(steam.join("steamapps/libraryfolders.vdf"))
        .is_ok_and(|metadata| metadata.len() <= 1_048_576)
        && let Ok(directory) = steamlocate::SteamDir::from_dir(steam)
        && let Ok(paths) = directory.library_paths()
    {
        libraries.extend(paths.into_iter().filter(|path| path.is_absolute()));
    }
    libraries.sort();
    libraries.dedup();
    libraries
        .into_iter()
        .flat_map(|path| {
            [
                path.join("steamapps/compatdata"),
                path.join("steamapps/common/World of Warcraft"),
            ]
        })
        .collect()
}
fn varint(bytes: &[u8], offset: &mut usize) -> Option<u64> {
    let mut value = 0;
    for shift in (0..70).step_by(7) {
        let byte = *bytes.get(*offset)?;
        *offset += 1;
        if shift == 63 && byte > 1 {
            return None;
        }
        value |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Some(value);
        }
    }
    None
}
fn fields(bytes: &[u8]) -> Option<Vec<(u64, &[u8])>> {
    let mut result = vec![];
    let mut offset = 0;
    while offset < bytes.len() {
        let tag = varint(bytes, &mut offset)?;
        if tag >> 3 == 0 {
            return None;
        }
        let length = match tag & 7 {
            0 => {
                varint(bytes, &mut offset)?;
                continue;
            }
            1 => 8,
            2 => usize::try_from(varint(bytes, &mut offset)?).ok()?,
            5 => 4,
            _ => return None,
        };
        let end = offset.checked_add(length)?;
        let data = bytes.get(offset..end)?;
        if tag & 7 == 2 {
            result.push((tag >> 3, data));
        }
        offset = end;
    }
    Some(result)
}
fn product_paths(bytes: &[u8]) -> Vec<&str> {
    let Some(products) = fields(bytes) else {
        return vec![];
    };
    products
        .into_iter()
        .filter(|(field, _)| *field == 1)
        .take(128)
        .filter_map(|(_, product)| {
            let product = fields(product)?;
            let code = product.iter().find(|(field, _)| *field == 2)?.1;
            if !matches!(code, b"wow" | b"wow_classic" | b"wow_classic_era") {
                return None;
            }
            let settings = fields(product.iter().find(|(field, _)| *field == 3)?.1)?;
            std::str::from_utf8(settings.iter().find(|(field, _)| *field == 1)?.1).ok()
        })
        .collect()
}
pub fn battle_net_roots(database: &Path, prefix: Option<&Path>) -> Vec<PathBuf> {
    let Some(bytes) = read_small(database) else {
        return vec![];
    };
    product_paths(&bytes)
        .into_iter()
        .filter_map(|value| resolve_path(value, prefix))
        .collect()
}
fn resolve_path(value: &str, prefix: Option<&Path>) -> Option<PathBuf> {
    if let Some(prefix) = prefix {
        let normalized = value.replace('\\', "/");
        let bytes = normalized.as_bytes();
        if bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && &bytes[1..3] == b":/" {
            let drive = char::from(bytes[0]).to_ascii_lowercase();
            let root = if drive == 'c' {
                prefix.join("drive_c")
            } else {
                prefix.join("dosdevices").join(format!("{drive}:"))
            };
            return Some(root.join(&normalized[3..]));
        }
        // A Windows path without a drive is not an absolute host path.
        return None;
    }
    let path = PathBuf::from(value);
    path.is_absolute().then_some(path)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn field(number: u8, value: &[u8]) -> Vec<u8> {
        assert!(value.len() < 128);
        let mut result = vec![
            (number << 3) | 2,
            u8::try_from(value.len()).expect("length"),
        ];
        result.extend_from_slice(value);
        result
    }
    #[test]
    fn steam_missing_malformed_and_oversized_metadata_keep_local_prefixes() {
        let root = tempfile::tempdir().expect("root");
        let expected = vec![
            root.path().join("steamapps/compatdata"),
            root.path().join("steamapps/common/World of Warcraft"),
        ];
        assert_eq!(steam_roots(root.path()), expected);
        std::fs::create_dir_all(root.path().join("steamapps")).expect("steamapps");
        let metadata = root.path().join("steamapps/libraryfolders.vdf");
        std::fs::write(&metadata, "\"libraryfolders\" { malformed").expect("metadata");
        assert_eq!(steam_roots(root.path()), expected);
        std::fs::write(&metadata, vec![b' '; 1_048_577]).expect("oversized");
        assert_eq!(steam_roots(root.path()), expected);
    }
    #[test]
    fn battle_net_reads_only_wow_and_rejects_truncated_database() {
        let mut product = field(2, b"wow");
        product.extend(field(3, &field(1, br"D:\Games\World of Warcraft")));
        let database = field(1, &product);
        assert_eq!(product_paths(&database), [r"D:\Games\World of Warcraft"]);
        assert!(product_paths(&database[..database.len() - 1]).is_empty());
        assert_eq!(
            resolve_path(r"C:\Games\WoW", Some(Path::new("/prefix"))),
            Some(PathBuf::from("/prefix/drive_c/Games/WoW"))
        );
        assert_eq!(
            resolve_path(r"D:\Games\WoW", Some(Path::new("/prefix"))),
            Some(PathBuf::from("/prefix/dosdevices/d:/Games/WoW"))
        );
        assert!(resolve_path("relative/path", None).is_none());
        assert!(fields(&[0xff; 10]).is_none());
        assert!(fields(&[0]).is_none());
        let mut other = field(2, b"other-game");
        other.extend(field(3, &field(1, b"C:/Games/Other")));
        assert!(product_paths(&field(1, &other)).is_empty());
    }
}
