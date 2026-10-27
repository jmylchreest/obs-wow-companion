//! Transactional library and durable capture checkpoints.
use anyhow::{Context, Result, bail};
use owc_domain::status::RecordingStatus;
use owc_domain::{Activity, ClipPlan, Segment};
use rusqlite::{Connection, params};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub struct Library {
    db: Connection,
}
impl Library {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Connection::open(path)?;
        db.busy_timeout(std::time::Duration::from_secs(3))?;
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version > 4 {
            bail!("library was created by a newer version; refusing to modify it");
        }
        db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
          CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT NOT NULL);
          CREATE TABLE IF NOT EXISTS activities(id TEXT PRIMARY KEY,data TEXT NOT NULL,status TEXT NOT NULL DEFAULT 'pending',path TEXT,plan TEXT,error TEXT,protected INTEGER NOT NULL DEFAULT 0,tags TEXT NOT NULL DEFAULT '[]',audio_tracks INTEGER NOT NULL DEFAULT 0,origin_ms INTEGER);
          CREATE TABLE IF NOT EXISTS segments(id TEXT PRIMARY KEY,data TEXT NOT NULL);
          UPDATE activities SET status='pending',error='Recovered interrupted export' WHERE status='exporting';")?;
        if version < 2 {
            db.execute_batch("BEGIN; ALTER TABLE activities ADD COLUMN precise INTEGER NOT NULL DEFAULT 0; PRAGMA user_version=2; COMMIT;")?;
        }
        db.execute_batch("BEGIN;
          CREATE TABLE IF NOT EXISTS sidecar_updates(id TEXT PRIMARY KEY REFERENCES activities(id) ON DELETE CASCADE);
          DROP TRIGGER IF EXISTS sidecar_activity_insert;
          DROP TRIGGER IF EXISTS sidecar_activity_update;
          CREATE TRIGGER sidecar_activity_insert AFTER INSERT ON activities WHEN NEW.status='ready'
            BEGIN INSERT INTO sidecar_updates SELECT NEW.id WHERE NOT EXISTS (SELECT 1 FROM sidecar_updates WHERE id=NEW.id); END;
          CREATE TRIGGER sidecar_activity_update AFTER UPDATE OF data,tags,protected,status,audio_tracks,origin_ms ON activities WHEN NEW.status='ready'
            BEGIN INSERT INTO sidecar_updates SELECT NEW.id WHERE NOT EXISTS (SELECT 1 FROM sidecar_updates WHERE id=NEW.id); END;
          INSERT OR IGNORE INTO sidecar_updates SELECT id FROM activities WHERE status='ready';
          CREATE TABLE IF NOT EXISTS latest_bundles(id TEXT PRIMARY KEY REFERENCES activities(id) ON DELETE CASCADE,data TEXT NOT NULL);
          PRAGMA user_version=4; COMMIT;")?;
        Ok(Self { db })
    }
    pub fn pending_sidecars(&self) -> Result<Vec<String>> {
        let mut query = self.db.prepare("SELECT s.id FROM sidecar_updates s JOIN activities a ON a.id=s.id WHERE a.status='ready'")?;
        Ok(query
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn sidecar_synced(&self, id: &str) -> Result<()> {
        self.db
            .execute("DELETE FROM sidecar_updates WHERE id=?", [id])?;
        Ok(())
    }
    pub fn setting(&self, key: &str) -> Result<Option<Value>> {
        use rusqlite::OptionalExtension;
        self.db
            .query_row("SELECT value FROM settings WHERE key=?", [key], |r| {
                r.get::<_, String>(0)
            })
            .optional()?
            .map(|s| serde_json::from_str(&s).context("decode setting"))
            .transpose()
    }
    pub fn set_setting(&self, key: &str, value: &Value) -> Result<()> {
        self.db.execute("INSERT INTO settings VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value.to_string()])?;
        Ok(())
    }
    pub fn checkpoint(&mut self, key: &str, state: &Value, activities: &[Activity]) -> Result<()> {
        let tx = self.db.transaction()?;
        tx.execute("INSERT INTO settings VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,state.to_string()])?;
        for a in activities {
            tx.execute("INSERT INTO activities(id,data) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",params![a.id.to_string(),serde_json::to_string(a)?])?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn save_activity(&mut self, a: &Activity) -> Result<()> {
        self.checkpoint("last-write", &Value::Null, std::slice::from_ref(a))
    }
    pub fn add_segment(&self, s: &Segment) -> Result<()> {
        self.db.execute(
            "INSERT INTO segments VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data",
            params![s.id.to_string(), serde_json::to_string(s)?],
        )?;
        Ok(())
    }
    pub fn segments(&self) -> Result<Vec<Segment>> {
        let mut st = self.db.prepare("SELECT data FROM segments")?;
        let rows = st.query_map([], |r| r.get::<_, String>(0))?;
        rows.map(|s| Ok(serde_json::from_str(&s?)?)).collect()
    }
    pub fn remove_segment(&self, id: &str) -> Result<()> {
        self.db.execute("DELETE FROM segments WHERE id=?", [id])?;
        Ok(())
    }
    pub fn save_latest_bundle(&self, id: &str, details: &Value) -> Result<()> {
        self.db.execute("INSERT INTO latest_bundles(id,data) VALUES(?1,?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data", params![id,details.to_string()])?;
        Ok(())
    }
    pub fn list(&self) -> Result<Vec<Value>> {
        let mut st=self.db.prepare("SELECT data,status,path,error,protected,tags,audio_tracks,origin_ms,(SELECT data FROM latest_bundles b WHERE b.id=activities.id) FROM activities ORDER BY json_extract(data,'$.start_ms') DESC")?;
        let rows = st.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, bool>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, u32>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, Option<String>>(8)?,
            ))
        })?;
        rows.map(|row| {
            let (data, status, path, error, protected, tags, audio, origin, bundle) = row?;
            let mut v: Value = serde_json::from_str(&data)?;
            v["status"] = status.into();
            v["path"] = path.into();
            v["error"] = error.into();
            v["protected"] = protected.into();
            v["tags"] = serde_json::from_str(&tags)?;
            v["audio_tracks"] = audio.into();
            v["origin_ms"] = origin.into();
            v["latest_bundle"] = bundle
                .map(|data| serde_json::from_str(&data))
                .transpose()?
                .unwrap_or(Value::Null);
            Ok(v)
        })
        .collect()
    }
    pub fn activity(&self, id: &str) -> Result<Activity> {
        let s: String = self
            .db
            .query_row("SELECT data FROM activities WHERE id=?", [id], |r| r.get(0))?;
        Ok(serde_json::from_str(&s)?)
    }
    pub fn pending(&self) -> Result<Vec<Activity>> {
        let mut st = self
            .db
            .prepare("SELECT data FROM activities WHERE status='pending'")?;
        st.query_map([], |r| r.get::<_, String>(0))?
            .map(|s| Ok(serde_json::from_str(&s?)?))
            .collect()
    }
    pub fn export_plan(&self, id: &str) -> Result<Option<(ClipPlan, bool)>> {
        let (plan, precise): (Option<String>, bool) = self.db.query_row(
            "SELECT plan,precise FROM activities WHERE id=?",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        plan.map(|p| Ok((serde_json::from_str(&p)?, precise)))
            .transpose()
    }
    pub fn begin_export(&self, plan: &ClipPlan, path: &Path, precise: bool) -> Result<()> {
        self.db.execute("UPDATE activities SET status='exporting',plan=?2,path=?3,origin_ms=?4,precise=?5,error=NULL WHERE id=?1",params![plan.activity_id.to_string(),serde_json::to_string(plan)?,path.to_string_lossy(),plan.actual_start_ms,precise])?;
        Ok(())
    }
    pub fn finish_export(&self, id: &str, audio: usize) -> Result<()> {
        self.db.execute(
            "UPDATE activities SET status='ready',audio_tracks=?2,error=NULL WHERE id=?1",
            params![id, i64::try_from(audio)?],
        )?;
        Ok(())
    }
    pub fn fail(&self, id: &str, status: RecordingStatus, error: &str) -> Result<()> {
        self.db.execute(
            "UPDATE activities SET status=?2,error=?3 WHERE id=?1",
            params![id, status.as_str(), error],
        )?;
        Ok(())
    }
    pub fn retry(&self, id: &str) -> Result<()> {
        self.db.execute("UPDATE activities SET status='pending',error=NULL WHERE id=? AND status IN ('failed','unavailable')",[id])?;
        Ok(())
    }
    pub fn retry_recovery(&self, id: &str) -> Result<()> {
        // A recovered source set supersedes any plan referencing vanished files.
        self.db.execute("UPDATE activities SET status='pending',plan=NULL,error=NULL WHERE id=? AND status IN ('pending','failed','unavailable')", [id])?;
        Ok(())
    }
    pub fn annotate(&self, id: &str, protected: bool, tags: &[String]) -> Result<()> {
        if tags.len() > 30 || tags.iter().any(|s| s.len() > 128) {
            bail!("too many or oversized tags");
        }
        self.db.execute(
            "UPDATE activities SET protected=?2,tags=?3 WHERE id=?1",
            params![id, protected, serde_json::to_string(tags)?],
        )?;
        Ok(())
    }
    pub fn media_path(&self, id: &str) -> Result<PathBuf> {
        let path: String = self.db.query_row(
            "SELECT path FROM activities WHERE id=? AND status='ready'",
            [id],
            |r| r.get(0),
        )?;
        Ok(path.into())
    }
    pub fn deletable_path(&self, id: &str) -> Result<Option<PathBuf>> {
        let mut st = self
            .db
            .prepare("SELECT path,protected,status FROM activities WHERE id=?")?;
        let (path, protected, status): (Option<String>, bool, String) =
            st.query_row([id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
        if protected || status == "exporting" {
            bail!("recording is protected or exporting");
        }
        if let Some(path) = &path {
            let mut pending = self.db.prepare("SELECT plan FROM activities WHERE plan IS NOT NULL AND status IN ('pending','exporting','failed')")?;
            for plan in pending.query_map([], |r| r.get::<_, String>(0))? {
                let plan: ClipPlan = serde_json::from_str(&plan?)?;
                if plan
                    .sources
                    .iter()
                    .any(|source| source.segment.path == Path::new(path))
                {
                    bail!(
                        "recording is a source for an unfinished export; finish or delete that job first"
                    );
                }
            }
        }
        Ok(path.map(PathBuf::from))
    }
    pub fn delete(&self, id: &str) -> Result<Option<PathBuf>> {
        let path = self.deletable_path(id)?;
        self.db.execute("DELETE FROM activities WHERE id=?", [id])?;
        Ok(path)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use owc_domain::{Kind, Outcome};
    #[test]
    #[ignore = "requires private 4 MiB tmpfs; run scripts/disk-full-smoke.py"]
    fn disk_full_checkpoint_rolls_back_cursor_and_activity_together() {
        let root = PathBuf::from(std::env::var("OWC_LIMITED_FILESYSTEM").expect("private tmpfs"));
        let path = root.join("library.sqlite");
        let mut db = Library::open(&path).expect("open");
        let activity = Activity::new(Kind::Manual, "Original".into(), 0, "manual");
        db.checkpoint(
            "logs",
            &serde_json::json!({"offset":10}),
            std::slice::from_ref(&activity),
        )
        .expect("initial checkpoint");
        let mut update = activity.clone();
        update.name = "x".repeat(8 * 1024 * 1024);
        let error = db
            .checkpoint("logs", &serde_json::json!({"offset":20}), &[update])
            .expect_err("filesystem full");
        assert!(
            matches!(error.downcast_ref::<rusqlite::Error>(),
            Some(rusqlite::Error::SqliteFailure(error, _)) if error.code == rusqlite::ErrorCode::DiskFull),
            "{error:#}"
        );
        assert_eq!(
            db.setting("logs").expect("cursor"),
            Some(serde_json::json!({"offset":10}))
        );
        assert_eq!(
            db.activity(&activity.id.to_string())
                .expect("activity")
                .name,
            "Original"
        );
        drop(db);
        let mut db = Library::open(&path).expect("reopen after rollback");
        assert_eq!(
            db.setting("logs").expect("durable cursor"),
            Some(serde_json::json!({"offset":10}))
        );
        assert_eq!(db.list().expect("durable activities").len(), 1);
        let mut update = activity;
        update.name = "Recovered".into();
        db.checkpoint(
            "logs",
            &serde_json::json!({"offset":20}),
            std::slice::from_ref(&update),
        )
        .expect("retry smaller transaction");
        assert_eq!(
            db.activity(&update.id.to_string())
                .expect("updated activity")
                .name,
            "Recovered"
        );
        assert_eq!(
            db.setting("logs").expect("advanced cursor"),
            Some(serde_json::json!({"offset":20}))
        );
        let integrity: String = db
            .db
            .query_row("PRAGMA integrity_check", [], |row| row.get(0))
            .expect("integrity");
        assert_eq!(integrity, "ok");
    }

    #[test]
    fn locked_checkpoint_is_bounded_and_retries_without_losing_activity() {
        let root = tempfile::tempdir().expect("temp");
        let path = root.path().join("library.sqlite");
        let mut db = Library::open(&path).expect("open");
        db.checkpoint("logs", &serde_json::json!({"offset":10}), &[])
            .expect("initial cursor");
        let other = Connection::open(&path).expect("competing connection");
        other.execute_batch("BEGIN IMMEDIATE").expect("writer lock");
        let activity = Activity::new(Kind::Manual, "Pending".into(), 0, "manual");
        let started = std::time::Instant::now();
        let error = db
            .checkpoint(
                "logs",
                &serde_json::json!({"offset":20}),
                std::slice::from_ref(&activity),
            )
            .expect_err("locked writer");
        assert!(started.elapsed() < std::time::Duration::from_secs(10));
        assert!(
            matches!(error.downcast_ref::<rusqlite::Error>(),
            Some(rusqlite::Error::SqliteFailure(error, _)) if error.code == rusqlite::ErrorCode::DatabaseBusy),
            "{error:#}"
        );
        assert_eq!(
            db.setting("logs").expect("unchanged cursor"),
            Some(serde_json::json!({"offset":10}))
        );
        assert!(db.list().expect("no partial activity").is_empty());
        other.execute_batch("ROLLBACK").expect("release lock");
        db.checkpoint(
            "logs",
            &serde_json::json!({"offset":20}),
            std::slice::from_ref(&activity),
        )
        .expect("retry retained batch");
        drop(db);
        let db = Library::open(&path).expect("reopen");
        assert_eq!(
            db.setting("logs").expect("durable cursor"),
            Some(serde_json::json!({"offset":20}))
        );
        assert_eq!(db.list().expect("exactly one activity").len(), 1);
        assert_eq!(
            db.activity(&activity.id.to_string())
                .expect("durable activity")
                .name,
            "Pending"
        );
    }

    #[test]
    fn latest_bundle_survives_restart_replaces_previous_and_cascades_on_delete() {
        let root = tempfile::tempdir().expect("temp");
        let path = root.path().join("library.sqlite");
        let mut db = Library::open(&path).expect("open");
        let activity = Activity::new(owc_domain::Kind::Manual, "test".into(), 0, "manual");
        db.save_activity(&activity).expect("activity");
        let id = activity.id.to_string();
        db.save_latest_bundle(&id, &serde_json::json!({"path":"first.zip"}))
            .expect("save");
        db.save_latest_bundle(
            &id,
            &serde_json::json!({"path":"second.zip","video":{"duration_ms":1234}}),
        )
        .expect("replace");
        drop(db);
        let db = Library::open(&path).expect("reopen");
        assert_eq!(
            db.list().expect("list")[0]["latest_bundle"]["path"],
            "second.zip"
        );
        db.delete(&id).expect("delete");
        let count: u32 = db
            .db
            .query_row("SELECT COUNT(*) FROM latest_bundles", [], |row| row.get(0))
            .expect("count");
        assert_eq!(count, 0);
    }
    #[test]
    fn failed_trim_keeps_its_source_pinned() {
        let dir = tempfile::tempdir().expect("directory");
        let mut db = Library::open(&dir.path().join("db")).expect("database");
        let mut original = Activity::new(Kind::Manual, "Original".into(), 0, "manual");
        original.finish(3000, Outcome::Unknown);
        db.save_activity(&original).expect("save");
        let path = dir.path().join("source.mkv");
        let original_plan = owc_domain::plan_clip(&original, &[], 0, 0);
        db.begin_export(&original_plan, &path, true).expect("job");
        db.finish_export(&original.id.to_string(), 2)
            .expect("ready");
        let mut trim = Activity::new(Kind::Manual, "Trim".into(), 1000, "clip");
        trim.finish(2000, Outcome::Unknown);
        db.save_activity(&trim).expect("save");
        let source = Segment {
            id: original.id,
            session: original.id,
            path: path.clone(),
            start_ms: 0,
            end_ms: 3000,
            closed: true,
            managed: false,
        };
        let plan = owc_domain::plan_clip(&trim, &[source], 0, 0);
        db.begin_export(&plan, &dir.path().join("trim.mkv"), true)
            .expect("job");
        db.fail(
            &trim.id.to_string(),
            RecordingStatus::Failed,
            "simulated disk full",
        )
        .expect("failure");
        assert!(db.deletable_path(&original.id.to_string()).is_err());
        db.delete(&trim.id.to_string()).expect("remove failed job");
        assert_eq!(
            db.deletable_path(&original.id.to_string())
                .expect("unpinned"),
            Some(path)
        );
    }
    #[test]
    fn interrupted_precise_export_preserves_plan() {
        let dir = tempfile::tempdir().expect("directory");
        let path = dir.path().join("db.sqlite");
        let mut db = Library::open(&path).expect("database");
        let mut activity = Activity::new(Kind::Manual, "Trim".into(), 1000, "clip");
        activity.finish(2000, Outcome::Unknown);
        db.save_activity(&activity).expect("save");
        let plan = owc_domain::plan_clip(&activity, &[], 0, 0);
        db.begin_export(&plan, &dir.path().join("clip.mkv"), true)
            .expect("job");
        drop(db);
        let db = Library::open(&path).expect("recover");
        assert_eq!(db.pending().expect("pending").len(), 1);
        let (recovered, precise) = db
            .export_plan(&activity.id.to_string())
            .expect("plan")
            .expect("persisted");
        assert!(precise);
        assert_eq!(
            serde_json::to_value(recovered).expect("json"),
            serde_json::to_value(plan).expect("json")
        );
    }
    #[test]
    fn refuses_future_schema_without_rewriting_it() {
        let dir = tempfile::tempdir().expect("directory");
        let path = dir.path().join("db.sqlite");
        let db = Connection::open(&path).expect("database");
        db.execute_batch("PRAGMA user_version=99").expect("version");
        assert!(Library::open(&path).is_err());
        assert_eq!(
            db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                .expect("version"),
            99
        );
    }
    #[test]
    fn recovery_protection_and_checkpoint_are_persistent() {
        let dir = tempfile::tempdir().expect("dir");
        let path = dir.path().join("db.sqlite");
        let mut db = Library::open(&path).expect("open");
        let mut a = Activity::new(Kind::Raid, "Boss".into(), 0, "r");
        a.finish(10, Outcome::Success);
        db.checkpoint("cursor", &serde_json::json!({"offset":17}), &[a.clone()])
            .expect("checkpoint");
        db.annotate(&a.id.to_string(), true, &["keep".into()])
            .expect("tag");
        let clip = dir.path().join("clip.mkv");
        db.begin_export(&owc_domain::plan_clip(&a, &[], 0, 0), &clip, true)
            .expect("export job");
        db.finish_export(&a.id.to_string(), 2).expect("published");
        assert!(db.delete(&a.id.to_string()).is_err());
        drop(db);
        let mut db = Library::open(&path).expect("reopen");
        db.checkpoint("cursor", &serde_json::json!({"offset":17}), &[a])
            .expect("replayed completion");
        assert_eq!(
            db.setting("cursor").expect("setting").expect("present")["offset"],
            17
        );
        let records = db.list().expect("list");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["protected"], true);
        assert_eq!(records[0]["tags"], serde_json::json!(["keep"]));
        assert_eq!(records[0]["status"], "ready");
        assert_eq!(records[0]["path"], clip.to_string_lossy().as_ref());
        assert_eq!(records[0]["audio_tracks"], 2);
        assert!(db.pending().expect("queue").is_empty());
    }
}
