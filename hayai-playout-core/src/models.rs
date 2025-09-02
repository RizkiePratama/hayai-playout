use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlaylistItem {
    pub id: u64,
    pub uri: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncodingSettings {
    pub encoder: String,
    pub bitrate_kbps: u32,
    pub speed_preset: String,
    pub scale_enabled: bool,
    pub scale_width: u32,
    pub scale_height: u32,
}

impl Default for EncodingSettings {
    fn default() -> Self {
        Self {
            encoder: "x264enc".to_string(),
            bitrate_kbps: 4000,
            speed_preset: "ultrafast".to_string(),
            scale_enabled: false,
            scale_width: 1920,
            scale_height: 1080,
        }
    }
}