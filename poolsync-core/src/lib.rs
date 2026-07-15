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
        self.kvm_enabled
            .unwrap_or(matches!(self.mode, AgentMode::Full))
    }

    pub fn kvm_capture_active(&self) -> bool {
        self.kvm_capture.unwrap_or(self.kvm_active())
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
    MouseMove {
        x: i32,
        y: i32,
    },
    MouseMoveRelative {
        dx: i32,
        dy: i32,
    },
    MouseButton {
        button: u8,
        pressed: bool,
        x: i32,
        y: i32,
    },
    MouseWheel {
        delta: i32,
        x: i32,
        y: i32,
    },
    Key {
        keycode: u32,
        pressed: bool,
    },
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(mode: AgentMode, kvm_enabled: Option<bool>, kvm_capture: Option<bool>) -> AgentConfig {
        AgentConfig {
            node: "n".into(),
            hub_url: "ws://x/ws".into(),
            token: "t".into(),
            mode,
            screen: ScreenInfo {
                width: 100,
                height: 100,
            },
            neighbors: vec![],
            edge_px: default_edge_px(),
            clipboard_poll_ms: default_poll_ms(),
            input_poll_ms: default_input_poll_ms(),
            pause_clipboard_when_rdp: true,
            display: None,
            kvm_enabled,
            kvm_capture,
        }
    }

    #[test]
    fn kvm_active_defaults_from_mode() {
        assert!(cfg(AgentMode::Full, None, None).kvm_active());
        assert!(!cfg(AgentMode::ClipboardOnly, None, None).kvm_active());
    }

    #[test]
    fn kvm_enabled_overrides_mode() {
        assert!(!cfg(AgentMode::Full, Some(false), None).kvm_active());
        assert!(cfg(AgentMode::ClipboardOnly, Some(true), None).kvm_active());
    }

    #[test]
    fn kvm_capture_falls_back_to_kvm_active() {
        let c = cfg(AgentMode::Full, None, None);
        assert_eq!(c.kvm_capture_active(), c.kvm_active());
        assert!(!cfg(AgentMode::Full, None, Some(false)).kvm_capture_active());
    }

    #[test]
    fn message_round_trip() {
        let msg = Message::Clipboard {
            msg_id: "id".into(),
            hash: "h".into(),
            mime: "text/plain".into(),
            data: "hello".into(),
        };
        let raw = encode_message(&msg).unwrap();
        match decode_message(&raw).unwrap() {
            Message::Clipboard { data, mime, .. } => {
                assert_eq!(data, "hello");
                assert_eq!(mime, "text/plain");
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    #[test]
    fn input_kind_tag_encoding() {
        let raw = encode_message(&Message::Input {
            target: "n".into(),
            kind: InputKind::Key {
                keycode: 65,
                pressed: true,
            },
        })
        .unwrap();
        assert!(raw.contains("\"type\":\"input\""), "{raw}");
        assert!(raw.contains("\"kind\":\"key\""), "{raw}");
    }

    #[test]
    fn hash_is_stable_and_hex() {
        // SHA-256("abc")
        assert_eq!(
            hash_text("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn topology_default_is_empty() {
        assert!(PoolTopology::default().nodes.is_empty());
    }
}
