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
    /// true = synchroniser aussi text/html (gras, etc.). false = texte brut uniquement.
    #[serde(default)]
    pub keep_formatting: bool,
    /// Double-clic dans l'historique presse-papiers → coller sur ce poste.
    #[serde(default)]
    pub history_double_click_paste: bool,
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

/// Un moniteur physique, tel que RandR le rapporte (coordonnées du bureau X11).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorInfo {
    /// Nom de la sortie RandR (« eDP-1 », « HDMI-1 »…), pour l'affichage.
    #[serde(default)]
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    /// Moniteur primaire RandR — celui où l'utilisateur travaille.
    #[serde(default)]
    pub primary: bool,
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
        /// Synchro presse-papiers active sur ce nœud (case du menu systray).
        /// Un nœud « sourd » est invisible sans cette information : c'est ce qui
        /// a coûté une matinée de diagnostic le 02/09.
        #[serde(default = "default_true")]
        clipboard_sync: bool,
        /// PoolSync actif localement (raccourci de pause Ctrl+Alt+Shift+P).
        #[serde(default = "default_true")]
        local_active: bool,
        /// Tous les moniteurs RandR actifs, pour la mosaïque multi-écrans.
        /// Vide = agent d'une version antérieure : le hub retombe sur `screen`.
        #[serde(default)]
        monitors: Vec<MonitorInfo>,
    },
    Clipboard {
        msg_id: String,
        hash: String,
        mime: String,
        data: String,
        /// Nœud où la copie a réellement eu lieu (pas le dernier relais).
        /// Vide = émetteur d'une version antérieure au horodatage logique.
        #[serde(default)]
        origin: String,
        /// Horloge logique (Lamport) de `origin` au moment de la copie.
        /// Donne un ordre total sur le mesh : plus besoin de fenêtres de grâce.
        /// 0 = message legacy, appliqué sans contrôle d'ordre.
        #[serde(default)]
        seq: u64,
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
            keep_formatting: false,
            history_double_click_paste: false,
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
            origin: "asus".into(),
            seq: 42,
        };
        let raw = encode_message(&msg).unwrap();
        match decode_message(&raw).unwrap() {
            Message::Clipboard {
                data, mime, origin, seq, ..
            } => {
                assert_eq!(data, "hello");
                assert_eq!(mime, "text/plain");
                assert_eq!(origin, "asus");
                assert_eq!(seq, 42);
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    /// Un agent d'une version antérieure n'envoie ni l'état de sa synchro ni
    /// ses moniteurs : le Hello doit rester décodable, et l'absence
    /// d'information ne doit pas le faire passer pour « sourd ».
    #[test]
    fn hello_from_an_older_agent_defaults_to_active() {
        let raw = r#"{"type":"hello","node":"acer","mode":"full",
            "screen":{"width":1366,"height":768},"neighbors":[]}"#;
        match decode_message(raw).unwrap() {
            Message::Hello {
                clipboard_sync,
                local_active,
                monitors,
                ..
            } => {
                assert!(clipboard_sync, "sans information, on suppose la synchro active");
                assert!(local_active);
                assert!(monitors.is_empty(), "le hub retombe alors sur `screen`");
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    /// Les moniteurs font l'aller-retour, drapeau primaire compris.
    #[test]
    fn monitors_round_trip_with_their_primary_flag() {
        let msg = Message::Hello {
            node: "asus".into(),
            mode: AgentMode::Full,
            screen: ScreenInfo { width: 1344, height: 756 },
            neighbors: vec![],
            kvm_enabled: true,
            kvm_desktop: KvmDesktopInfo::default(),
            clipboard_sync: false,
            local_active: true,
            monitors: vec![
                MonitorInfo { name: "eDP-1".into(), x: 1920, y: 614, width: 1344, height: 756, primary: true },
                MonitorInfo { name: "HDMI-1".into(), x: 0, y: 0, width: 1920, height: 1080, primary: false },
            ],
        };
        let raw = encode_message(&msg).unwrap();
        match decode_message(&raw).unwrap() {
            Message::Hello { clipboard_sync, monitors, .. } => {
                assert!(!clipboard_sync, "un nœud sourd doit être visible comme tel");
                assert_eq!(monitors.len(), 2);
                assert_eq!(monitors[0].name, "eDP-1");
                assert!(monitors[0].primary);
                assert!(!monitors[1].primary);
                assert_eq!(monitors[1].width, 1920);
            }
            other => panic!("wrong variant: {other:?}"),
        }
    }

    /// Un agent non encore mis à jour n'envoie ni `origin` ni `seq` : le
    /// message doit rester décodable, avec l'ordre neutre (0 / vide).
    #[test]
    fn clipboard_from_an_older_agent_still_decodes() {
        let raw = r#"{"type":"clipboard","msg_id":"id","hash":"h","mime":"text/plain","data":"hello"}"#;
        match decode_message(raw).unwrap() {
            Message::Clipboard { origin, seq, .. } => {
                assert!(origin.is_empty());
                assert_eq!(seq, 0);
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
