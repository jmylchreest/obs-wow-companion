//! Private crash-test helper; built by recovery-smoke.py, never shipped.
use std::{fs, io::Write, process::Command, time::Duration};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let executable = std::env::current_exe()?;
    let root = executable.parent().ok_or("worker directory missing")?;
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let part = args.iter().find(|arg| {
        std::path::Path::new(arg).file_name() == Some(std::ffi::OsStr::new("part-0000.mkv"))
    });
    if let Some(part) = part {
        let mut log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("worker-paths"))?;
        writeln!(log, "{}", part.to_string_lossy())?;
        match fs::remove_file(root.join("hold-once")) {
            Ok(()) => {
                fs::write(root.join("worker-started"), std::process::id().to_string())?;
                while !root.join("release-worker").exists() {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let status = Command::new("ffmpeg").args(args).status()?;
    // Signal completion only after FFmpeg exits, so the assertion cannot race
    // a late orphan write on a slow runner.
    if root.join("worker-started").exists()
        && fs::read_to_string(root.join("worker-started"))? == std::process::id().to_string()
    {
        fs::write(root.join("orphan-finished"), status.to_string())?;
    }
    std::process::exit(status.code().unwrap_or(1));
}
