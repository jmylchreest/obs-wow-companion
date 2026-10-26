use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{BufRead, Write},
    path::{Path, PathBuf},
};

const CAPACITY: usize = 500;
const MAX_BYTES: u64 = 1_048_576;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub at_ms: i64,
    pub source: String,
    pub level: String,
    pub message: String,
}
pub struct EventLog {
    pub entries: VecDeque<Entry>,
    pub error: Option<String>,
    path: PathBuf,
}
impl EventLog {
    pub fn new(root: &Path) -> Self {
        let path = root.join("events.jsonl");
        let mut log = Self {
            entries: VecDeque::new(),
            error: None,
            path,
        };
        for path in [root.join("events.previous.jsonl"), log.path.clone()] {
            if let Ok(file) = std::fs::File::open(path)
                && file
                    .metadata()
                    .is_ok_and(|meta| meta.len() <= MAX_BYTES + 8192)
            {
                for line in std::io::BufReader::new(file).lines().map_while(Result::ok) {
                    if let Ok(entry) = serde_json::from_str(&line) {
                        log.retain(entry);
                    }
                }
            }
        }
        log
    }
    fn retain(&mut self, entry: Entry) {
        self.entries.push_back(entry);
        while self.entries.len() > CAPACITY {
            self.entries.pop_front();
        }
    }
    pub fn push(&mut self, source: &str, level: &str, message: &str) {
        let entry = Entry {
            at_ms: chrono::Utc::now().timestamp_millis(),
            source: source.chars().take(100).collect(),
            level: level.into(),
            message: message.chars().take(1000).collect(),
        };
        if let Err(error) = self.append(&entry) {
            self.error = Some(format!("Could not save event log: {error}"));
        } else {
            self.error = None;
        }
        self.retain(entry);
    }
    fn append(&self, entry: &Entry) -> anyhow::Result<()> {
        if std::fs::metadata(&self.path).is_ok_and(|meta| meta.len() >= MAX_BYTES) {
            let previous = self.path.with_file_name("events.previous.jsonl");
            match std::fs::remove_file(&previous) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
            std::fs::rename(&self.path, previous)?;
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        serde_json::to_writer(&mut file, entry)?;
        file.write_all(b"\n")?;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_history_survives_restart_rotation_and_partial_lines() {
        let root = tempfile::tempdir().expect("root");
        let mut log = EventLog::new(root.path());
        for i in 0..510 {
            log.push("Retail", "info", &format!("Encounter {i}"));
        }
        assert_eq!(log.entries.len(), 500);
        assert_eq!(EventLog::new(root.path()).entries.len(), 500);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&log.path)
            .expect("file");
        file.write_all(b"{partial\n").expect("partial");
        file.set_len(MAX_BYTES).expect("size");
        log.push("OBS", "info", "Stopped");
        assert!(root.path().join("events.previous.jsonl").exists());
        assert_eq!(
            EventLog::new(root.path())
                .entries
                .back()
                .expect("entry")
                .message,
            "Stopped"
        );
    }
}
