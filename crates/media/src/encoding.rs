use serde::{Deserialize, Serialize};
use tokio::process::Command;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoder {
    #[default]
    Cpu,
    Nvidia,
    Intel,
    Amd,
    Vaapi,
}
impl Encoder {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Cpu => "libx264",
            Self::Nvidia => "h264_nvenc",
            Self::Intel => "h264_qsv",
            Self::Amd => "h264_amf",
            Self::Vaapi => "h264_vaapi",
        }
    }
    pub(crate) fn configure(self, command: &mut Command) {
        command.args(["-c:v", self.name()]);
        match self {
            Self::Cpu => {
                command.args(["-preset", "veryfast", "-crf", "20"]);
            }
            Self::Nvidia => {
                command.args(["-preset", "p4", "-rc", "vbr", "-cq", "20", "-b:v", "0"]);
            }
            Self::Intel => {
                command.args(["-preset", "veryfast", "-global_quality", "20"]);
            }
            Self::Amd => {
                command.args([
                    "-quality", "speed", "-rc", "cqp", "-qp_i", "20", "-qp_p", "20",
                ]);
            }
            Self::Vaapi => {
                command.args(["-vf", "format=nv12,hwupload", "-qp", "20"]);
            }
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportInfo {
    pub encoder: String,
    pub fallback: Option<String>,
}
