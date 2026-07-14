use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

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
    /// Avec RDP actif : court délai après collage hub (cohabitation cliprdr RDP).
    #[serde(default = "default_true")]
    pub pause_clipboard_when_rdp: bool,
    /// Display X11 (ex. ":10" pour session xrdp). Vide = auto via poolsync-agent-launch.sh
    #[serde(default)]
    pub display: Option<String>,
    /// Clavier/souris KVM (bords d'écran). Défaut : true si mode full, false si clipboard_only.
    #[serde(default)]
    pub kvm_enabled: Option<bool>,
    /// Capture souris/bords (primary Barrier). False = injection seule sur ce nœud.
    #[serde(default)]
    pub kvm_capture: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PoolTopology {
    #[serde(default)]
    pub nodes: HashMap<String, TopologyNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopologyNode {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    #[serde(default = "default_true")]
    pub kvm_enabled: bool,
    #[serde(default)]
    pub neighbors: HashMap<String, String>,
}

impl AgentConfig {
    pub fn kvm_active(&self) -> bool {
        self.kvm_enabled.unwrap_or(matches!(self.mode, AgentMode::Full))
    }

    pub fn kvm_capture_active(&self) -> bool {
        self.kvm_capture.unwrap_or(self.kvm_active())
    }
}

impl PoolTopology {
    pub fn default_now3() -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(
            "asus".into(),
            TopologyNode {
                x: 0,
                y: 0,
                width: 1344,
                height: 756,
                kvm_enabled: true,
                neighbors: [("right".into(), "acer".into())].into(),
            },
        );
        nodes.insert(
            "acer".into(),
            TopologyNode {
                x: 1344,
                y: 0,
                width: 1366,
                height: 768,
                kvm_enabled: true,
                neighbors: [
                    ("left".into(), "asus".into()),
                    ("right".into(), "inspiron".into()),
                ]
                .into(),
            },
        );
        nodes.insert(
            "inspiron".into(),
            TopologyNode {
                x: 2710,
                y: 0,
                width: 1366,
                height: 768,
                kvm_enabled: true,
                neighbors: [("left".into(), "acer".into())].into(),
            },
        );
        nodes.insert(
            "gbs-p2".into(),
            TopologyNode {
                x: 1920,
                y: 1080,
                width: 1920,
                height: 1080,
                kvm_enabled: false,
                neighbors: HashMap::new(),
            },
        );
        Self { nodes }
    }
}

fn default_true() -> bool {
    true
}

fn default_edge_px() -> u32 {
    5
}

fn default_poll_ms() -> u64 {
    400
}

fn default_input_poll_ms() -> u64 {
    8
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    Hello {
        node: String,
        mode: AgentMode,
        screen: ScreenInfo,
        neighbors: Vec<Neighbor>,
        #[serde(default)]
        kvm_enabled: bool,
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
        /// Machine qui possède clavier/souris physiques (modèle Barrier).
        #[serde(default)]
        input_node: String,
    },
    TopologyUpdate {
        topology: PoolTopology,
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
    MouseMoveRelative { dx: i32, dy: i32 },
    MouseButton { button: u8, pressed: bool, x: i32, y: i32 },
    MouseWheel { delta: i32, x: i32, y: i32 },
    Key { keycode: u32, pressed: bool },
}

pub fn hash_text(data: &str) -> String {
    hash_bytes(data.as_bytes())
}

pub fn hash_bytes(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    format!("{:x}", digest)
}

pub fn encode_message(msg: &Message) -> anyhow::Result<String> {
    Ok(serde_json::to_string(msg)?)
}

pub fn decode_message(raw: &str) -> anyhow::Result<Message> {
    Ok(serde_json::from_str(raw)?)
}
