use anyhow::{Context, Result};
use serde_json::Value;
use std::{io::Write, path::Path};

pub fn write(path: &Path, value: &Value) -> Result<()> {
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().context("Sidecar directory missing")?)?;
    serde_json::to_writer_pretty(&mut temporary, value)?;
    temporary.write_all(b"\n")?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub fn load(path: &Path) -> Result<Option<Value>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes).ok()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn update(
    path: &Path,
    activity: &owc_domain::Activity,
    record: &Value,
    mut data: Value,
) -> Result<()> {
    anyhow::ensure!(
        data["schema_version"] == 1,
        "Unsupported recording sidecar version"
    );
    data["activity"] = serde_json::to_value(activity)?;
    data["annotations"] =
        serde_json::json!({"tags":record["tags"],"protected":record["protected"]});
    if load(path)?.as_ref() != Some(&data) {
        write(path, &data)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edits_preserve_export_timing_and_replace_atomically() {
        let root = tempfile::tempdir().expect("root");
        let path = root.path().join("clip.json");
        let mut activity =
            owc_domain::Activity::new(owc_domain::Kind::Manual, "Clip".into(), 1000, "Retail");
        let original = serde_json::json!({"schema_version":1,"activity":activity,"plan":{"actual_start_ms":500},"media":{"duration_ms":3000},"extra":"preserved"});
        write(&path, &original).expect("initial");
        activity.marker(1500, "manual", "Check positioning".into());
        update(
            &path,
            &activity,
            &serde_json::json!({"tags":["review"],"protected":true}),
            original.clone(),
        )
        .expect("update");
        let saved: Value =
            serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("json");
        assert_eq!(saved["activity"]["markers"][0]["at_ms"], 1500);
        assert_eq!(saved["plan"], original["plan"]);
        assert_eq!(saved["media"], original["media"]);
        assert_eq!(saved["extra"], "preserved");
        assert_eq!(saved["annotations"]["protected"], true);
        assert_eq!(
            std::fs::read_dir(root.path()).expect("directory").count(),
            1
        );
    }
}
