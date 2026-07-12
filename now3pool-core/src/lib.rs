use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    Full,
    ClipboardOnly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenInfo {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Neighbor {
    pub direction: Direction,
    pub node: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub node: String,
    pub hub_url: String,
    pub token: String,
    pub mode: AgentMode,
    pub screen: ScreenInfo,
    #[serde(default)]
    pub neighbors: Vec<Neighbor>,
    #[serde(default = "default_edge_px")]
    pub edge_px: u32,
    #[serde(default = "default_poll_ms")]
    pub clipboard_poll_ms: u64,
    #[serde(default = "default_input_poll_ms")]
    pub input_poll_ms: u64,
}

fn default_edge_px() -> u32 {
    5
}

fn default_poll_ms() -> u64 {
    400
}

fn default_input_poll_ms() -> u64 {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Hello {
        node: String,
        mode: AgentMode,
        screen: ScreenInfo,
        neighbors: Vec<Neighbor>,
    },
    Clipboard {
        msg_id: String,
        hash: String,
        mime: String,
        data: String,
    },
    MasterClaim {
        node: String,
        ts: u64,
    },
    MasterChanged {
        node: String,
    },
    Input {
        target: String,
        kind: InputKind,
    },
    SwitchTo {
        node: String,
        x: i32,
        y: i32,
    },
    Ping,
    Pong,
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InputKind {
    MouseMove { x: i32, y: i32 },
    MouseButton { button: u8, pressed: bool, x: i32, y: i32 },
    MouseWheel { delta: i32, x: i32, y: i32 },
    Key { keycode: u32, pressed: bool },
}

pub fn hash_text(data: &str) -> String {
    let digest = Sha256::digest(data.as_bytes());
    format!("{:x}", digest)
}

pub fn encode_message(msg: &Message) -> anyhow::Result<String> {
    Ok(serde_json::to_string(msg)?)
}

pub fn decode_message(raw: &str) -> anyhow::Result<Message> {
    Ok(serde_json::from_str(raw)?)
}
