mod topology;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

pub use topology::{
    infer_neighbors, layout_scale, snap_position, DEFAULT_EDGE_TOLERANCE_PX, DEFAULT_SNAP_GRID_PX,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMode {
    Full,
    ClipboardOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScreenInfo {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Neighbor {
    pub direction: Direction,
    pub node: String,
    /// WebSocket direct LAN/VPN vers le voisin (clipboard sans relay hub).
    #[serde(default)]
    pub peer_url: Option<String>,
    /// Secours si peer_url (LAN) injoignable — ex. IP wg-bs1 du voisin.
    #[serde(default)]
    pub peer_url_vpn: Option<String>,
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
    /// Nombre d'entrées presse-papiers dans le menu systray.
    #[serde(default = "default_tray_history_count")]
    pub tray_history_count: u32,
    /// Port d'écoute WS peer-to-peer (clipboard direct entre voisins).
    #[serde(default = "default_peer_listen_port")]
    pub peer_listen_port: u16,
    /// Active le mesh clipboard direct vers les voisins configurés.
    #[serde(default = "default_true")]
    pub peer_direct_clipboard: bool,
    /// Relayer le presse-papiers via le hub (bs1). False = peer mesh only (pas d'upload blob vers le VPS).
    #[serde(default = "default_true")]
    pub hub_clipboard: bool,
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
    /// Moniteur primaire KVM (position X11 absolue).
    #[serde(default)]
    pub monitor_x: i32,
    #[serde(default)]
    pub monitor_y: i32,
    /// Origine du bureau X11 complet (tous écrans).
    #[serde(default)]
    pub desktop_x: i32,
    #[serde(default)]
    pub desktop_y: i32,
    /// Bureau X11 complet (tous écrans) — souris distante peut aller sur HDMI.
    #[serde(default)]
    pub desktop_width: u32,
    #[serde(default)]
    pub desktop_height: u32,
}

/// Géométrie bureau / moniteur primaire (partagée hub ↔ agents).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct KvmDesktopInfo {
    pub monitor_x: i32,
    pub monitor_y: i32,
    pub desktop_x: i32,
    pub desktop_y: i32,
    pub desktop_width: u32,
    pub desktop_height: u32,
}

impl KvmDesktopInfo {
    pub fn desktop_bounds(&self, primary: ScreenInfo) -> KvmDisplayRect {
        let (x, y, w, h) = if self.desktop_width > 0 && self.desktop_height > 0 {
            (
                self.desktop_x,
                self.desktop_y,
                self.desktop_width,
                self.desktop_height,
            )
        } else {
            (self.monitor_x, self.monitor_y, primary.width, primary.height)
        };
        KvmDisplayRect { x, y, width: w, height: h }
    }

    pub fn primary_bounds(&self, primary: ScreenInfo) -> KvmDisplayRect {
        KvmDisplayRect {
            x: self.monitor_x,
            y: self.monitor_y,
            width: primary.width,
            height: primary.height,
        }
    }

    pub fn desktop_size(&self, primary: ScreenInfo) -> ScreenInfo {
        ScreenInfo {
            width: if self.desktop_width > 0 {
                self.desktop_width
            } else {
                primary.width
            },
            height: if self.desktop_height > 0 {
                self.desktop_height
            } else {
                primary.height
            },
        }
    }
}

/// Rectangle écran X11 (pool ou bureau complet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvmDisplayRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl KvmDisplayRect {
    pub fn clamp(&self, px: i32, py: i32) -> (i32, i32) {
        (
            px.clamp(self.x, self.x + self.width as i32 - 1),
            py.clamp(self.y, self.y + self.height as i32 - 1),
        )
    }

    pub fn to_local(&self, px: i32, py: i32) -> (i32, i32) {
        (px - self.x, py - self.y)
    }

    pub fn to_root(&self, lx: i32, ly: i32) -> (i32, i32) {
        (lx + self.x, ly + self.y)
    }

    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && py >= self.y
            && px < self.x + self.width as i32
            && py < self.y + self.height as i32
    }
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

fn default_tray_history_count() -> u32 {
    15
}

fn default_peer_listen_port() -> u16 {
    9472
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
        #[serde(default)]
        kvm_desktop: KvmDesktopInfo,
    },
    Clipboard {
        msg_id: String,
        hash: String,
        mime: String,
        data: String,
    },
    /// Signal hub : l'historique presse-papiers a changé (menu systray / SSE).
    ClipboardHistoryUpdated {
        revision: u64,
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
            tray_history_count: default_tray_history_count(),
            peer_listen_port: default_peer_listen_port(),
            peer_direct_clipboard: true,
            hub_clipboard: true,
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
    fn clipboard_history_updated_round_trip() {
        let msg = Message::ClipboardHistoryUpdated { revision: 42 };
        let raw = encode_message(&msg).unwrap();
        match decode_message(&raw).unwrap() {
            Message::ClipboardHistoryUpdated { revision } => assert_eq!(revision, 42),
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
