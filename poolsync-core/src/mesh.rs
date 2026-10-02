//! Encrypted peer control and deterministic, expiring input ownership.
//! All clocks below are process-local monotonic milliseconds, not wall time.

use crate::{AgentMode, InputKind, KvmDesktopInfo, MonitorInfo, PoolTopology, ScreenInfo};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub const PRESENCE_TIMEOUT_MS: u64 = 4_000;
pub const CONTROL_TIMEOUT_MS: u64 = 3_000;
const DOMAIN: &[u8] = b"poolsync-peer-control-v1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Presence {
    pub mode: AgentMode,
    pub screen: ScreenInfo,
    pub desktop: KvmDesktopInfo,
    pub monitors: Vec<MonitorInfo>,
    pub active: bool,
    pub kvm: bool,
    #[serde(default)]
    pub control_clock: u64,
}

impl Presence {
    pub fn can_kvm(&self) -> bool {
        self.active && self.kvm && self.mode == AgentMode::Full
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Packet {
    pub id: String,
    pub origin: String,
    pub boot: String,
    pub seq: u64,
    pub sent_ms: u64,
    pub event: Event,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    Presence {
        presence: Presence,
    },
    Claim {
        term: u64,
    },
    Renew {
        term: u64,
        focus: String,
    },
    Switch {
        term: u64,
        target: String,
        x: i32,
        y: i32,
    },
    Input {
        term: u64,
        target: String,
        kind: InputKind,
    },
    Layout {
        revision: u64,
        author: String,
        topology: PoolTopology,
    },
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    peer_control: u8,
    nonce: String,
    ciphertext: String,
}

pub fn validate_key(key: &str) -> anyhow::Result<()> {
    crate::decode_e2e_key(key).map(|_| ())
}

pub fn encrypt(packet: &Packet, key: &str) -> anyhow::Result<String> {
    let key = crate::decode_e2e_key(key)?;
    let cipher = XChaCha20Poly1305::new((&key).into());
    let mut nonce = [0; 24];
    OsRng.fill_bytes(&mut nonce);
    let plain = serde_json::to_vec(packet)?;
    let ciphertext = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &plain,
                aad: DOMAIN,
            },
        )
        .map_err(|_| anyhow::anyhow!("peer control encryption failed"))?;
    Ok(serde_json::to_string(&Envelope {
        peer_control: 1,
        nonce: B64.encode(nonce),
        ciphertext: B64.encode(ciphertext),
    })?)
}

pub fn decrypt(wire: &str, key: &str) -> anyhow::Result<Packet> {
    let env: Envelope = serde_json::from_str(wire)?;
    anyhow::ensure!(env.peer_control == 1, "unsupported peer control version");
    let nonce: [u8; 24] = B64
        .decode(env.nonce)?
        .try_into()
        .map_err(|_| anyhow::anyhow!("invalid peer control nonce"))?;
    let cipher = XChaCha20Poly1305::new((&crate::decode_e2e_key(key)?).into());
    let ciphertext = B64.decode(env.ciphertext)?;
    let plain = cipher
        .decrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: &ciphertext,
                aad: DOMAIN,
            },
        )
        .map_err(|_| anyhow::anyhow!("peer control authentication failed"))?;
    Ok(serde_json::from_slice(&plain)?)
}

#[derive(Clone)]
pub struct Member {
    pub presence: Presence,
    pub boot: String,
    pub last_seen: u64,
    presence_seq: u64,
    seen: HashSet<u64>,
    max_seq: u64,
}

#[derive(Clone, Debug)]
pub struct Lease {
    pub owner: String,
    pub boot: String,
    pub term: u64,
    pub renewed_at: u64,
    pub focus: String,
    switch_seq: u64,
    input_seq: HashMap<String, u64>,
}

#[derive(Default)]
pub struct Control {
    pub members: HashMap<String, Member>,
    pub lease: Option<Lease>,
    pub clock: u64,
    // Remember retired boots, including while the member is absent. A delayed
    // packet from a dead process must not impersonate its restarted successor.
    retired: HashMap<String, HashSet<String>>,
    boots: HashMap<String, String>,
    last_claim: (u64, String),
}

impl Control {
    pub fn next_term(&mut self) -> u64 {
        self.clock = self.clock.saturating_add(1);
        self.clock
    }

    pub fn expire(&mut self, now: u64) {
        self.members
            .retain(|_, m| now.saturating_sub(m.last_seen) < PRESENCE_TIMEOUT_MS);
        if let Some(l) = &self.lease {
            if now.saturating_sub(l.renewed_at) >= CONTROL_TIMEOUT_MS
                || !self.enabled(&l.owner)
                || !self.enabled(&l.focus)
                || self.members.get(&l.owner).is_none_or(|m| m.boot != l.boot)
            {
                self.lease = None;
            }
        }
    }

    pub fn enabled(&self, node: &str) -> bool {
        self.members.get(node).is_some_and(|m| m.presence.can_kvm())
    }

    /// Returns true for a fresh, authorized event. Consumers may then relay it.
    /// Input events are accepted only for the current lease and focused target.
    pub fn accept(&mut self, packet: &Packet, now: u64) -> bool {
        self.expire(now);
        if packet.origin.is_empty()
            || packet.boot.is_empty()
            || packet.id.is_empty()
            || packet.seq == 0
        {
            return false;
        }
        if let Event::Presence { presence } = &packet.event {
            if presence.screen.width == 0
                || presence.screen.height == 0
                || presence.screen.width > 65535
                || presence.screen.height > 65535
                || self
                    .retired
                    .get(&packet.origin)
                    .is_some_and(|boots| boots.contains(&packet.boot))
            {
                return false;
            }
            if let Some(old_boot) = self.boots.get(&packet.origin) {
                if old_boot != &packet.boot {
                    self.retired
                        .entry(packet.origin.clone())
                        .or_default()
                        .insert(old_boot.clone());
                    self.members.remove(&packet.origin);
                    if self
                        .lease
                        .as_ref()
                        .is_some_and(|l| l.owner == packet.origin)
                    {
                        self.lease = None;
                    }
                }
            }
            self.boots
                .insert(packet.origin.clone(), packet.boot.clone());
            self.clock = self.clock.max(presence.control_clock);
            let m = self
                .members
                .entry(packet.origin.clone())
                .or_insert_with(|| Member {
                    presence: presence.clone(),
                    boot: packet.boot.clone(),
                    last_seen: now,
                    presence_seq: 0,
                    max_seq: 0,
                    seen: HashSet::new(),
                });
            if packet.seq <= m.presence_seq {
                return false;
            }
            m.presence_seq = packet.seq;
            m.presence = presence.clone();
            m.last_seen = now;
        }
        let Some(m) = self.members.get_mut(&packet.origin) else {
            return false;
        };
        if m.boot != packet.boot
            || packet.seq.saturating_add(256) < m.max_seq
            || !m.seen.insert(packet.seq)
        {
            return false;
        }
        m.max_seq = m.max_seq.max(packet.seq);
        let floor = m.max_seq.saturating_sub(256);
        m.seen.retain(|s| *s >= floor);
        match &packet.event {
            Event::Presence { .. } | Event::Layout { .. } => true,
            Event::Claim { term } => {
                if !self.enabled(&packet.origin)
                    || (*term, &packet.origin) <= (self.last_claim.0, &self.last_claim.1)
                {
                    return false;
                }
                self.clock = self.clock.max(*term);
                self.last_claim = (*term, packet.origin.clone());
                self.lease = Some(Lease {
                    owner: packet.origin.clone(),
                    boot: packet.boot.clone(),
                    term: *term,
                    renewed_at: now,
                    focus: packet.origin.clone(),
                    switch_seq: 0,
                    input_seq: HashMap::new(),
                });
                true
            }
            Event::Renew { term, focus } => {
                // A newly joined peer may learn an existing lease; an expired
                // lease already observed by this process cannot be resurrected.
                if (*term, &packet.origin) > (self.last_claim.0, &self.last_claim.1)
                    && self.enabled(&packet.origin)
                    && self.enabled(focus)
                {
                    self.clock = self.clock.max(*term);
                    self.last_claim = (*term, packet.origin.clone());
                    self.lease = Some(Lease {
                        owner: packet.origin.clone(),
                        boot: packet.boot.clone(),
                        term: *term,
                        renewed_at: now,
                        focus: focus.clone(),
                        switch_seq: packet.seq,
                        input_seq: HashMap::new(),
                    });
                    return true;
                }
                if let Some(l) = self.lease.as_mut().filter(|l| {
                    l.owner == packet.origin && l.boot == packet.boot && l.term == *term
                }) {
                    l.renewed_at = now;
                    true
                } else {
                    false
                }
            }
            Event::Switch { term, target, .. } => {
                if !self.enabled(target) {
                    return false;
                }
                if let Some(l) = self.lease.as_mut().filter(|l| {
                    l.owner == packet.origin
                        && l.boot == packet.boot
                        && l.term == *term
                        && packet.seq > l.switch_seq
                }) {
                    l.switch_seq = packet.seq;
                    l.focus = target.clone();
                    l.input_seq.clear();
                    true
                } else {
                    false
                }
            }
            Event::Input { term, target, kind } => {
                if !self.enabled(target) {
                    return false;
                }
                let Some(l) = self.lease.as_mut().filter(|l| {
                    l.owner == packet.origin
                        && l.boot == packet.boot
                        && l.term == *term
                        && l.focus == *target
                        && packet.seq > l.switch_seq
                }) else {
                    return false;
                };
                // Reordering motion must not suppress an unrelated key release.
                let stream = match kind {
                    InputKind::Key { keycode, .. } => format!("key:{keycode}"),
                    InputKind::MouseButton { button, .. } => format!("button:{button}"),
                    InputKind::MouseWheel { .. } => "wheel".into(),
                    _ => "motion".into(),
                };
                let last = l.input_seq.entry(stream).or_default();
                if packet.seq <= *last {
                    return false;
                }
                *last = packet.seq;
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn packet(node: &str, seq: u64, event: Event) -> Packet {
        Packet {
            id: format!("{node}:{seq}"),
            origin: node.into(),
            boot: "boot-1".into(),
            seq,
            sent_ms: 0,
            event,
        }
    }
    fn join(c: &mut Control, node: &str, full: bool) {
        assert!(c.accept(
            &packet(
                node,
                1,
                Event::Presence {
                    presence: Presence {
                        mode: if full {
                            AgentMode::Full
                        } else {
                            AgentMode::ClipboardOnly
                        },
                        screen: ScreenInfo {
                            width: 800,
                            height: 600
                        },
                        desktop: KvmDesktopInfo::default(),
                        monitors: vec![],
                        active: true,
                        kvm: full,
                        control_clock: 0,
                    }
                }
            ),
            0
        ));
    }
    #[test]
    fn simultaneous_claims_converge_in_both_arrival_orders() {
        for reverse in [false, true] {
            let mut c = Control::default();
            join(&mut c, "a", true);
            join(&mut c, "b", true);
            let a = packet("a", 2, Event::Claim { term: 1 });
            let b = packet("b", 2, Event::Claim { term: 1 });
            for p in if reverse { [&b, &a] } else { [&a, &b] } {
                c.accept(p, 10);
            }
            assert_eq!(c.lease.unwrap().owner, "b");
        }
    }
    #[test]
    fn owner_timeout_cannot_be_resurrected_by_a_late_renewal() {
        let mut c = Control::default();
        join(&mut c, "a", true);
        assert!(c.accept(&packet("a", 2, Event::Claim { term: 1 }), 1));
        c.expire(3_002);
        assert!(c.lease.is_none());
        assert!(!c.accept(
            &packet(
                "a",
                3,
                Event::Renew {
                    term: 1,
                    focus: "a".into()
                }
            ),
            3_003
        ));
        assert!(c.accept(&packet("a", 4, Event::Claim { term: 2 }), 3_004));
    }
    #[test]
    fn clipboard_only_peers_cannot_claim_or_receive_input() {
        let mut c = Control::default();
        join(&mut c, "a", true);
        join(&mut c, "c", false);
        assert!(!c.accept(&packet("c", 2, Event::Claim { term: 99 }), 1));
        assert!(c.accept(&packet("a", 2, Event::Claim { term: 1 }), 1));
        assert!(!c.accept(
            &packet(
                "a",
                3,
                Event::Switch {
                    term: 1,
                    target: "c".into(),
                    x: 1,
                    y: 1
                }
            ),
            2
        ));
    }
    #[test]
    fn old_process_and_wrong_lease_cannot_inject() {
        let mut c = Control::default();
        join(&mut c, "a", true);
        join(&mut c, "b", true);
        c.accept(&packet("a", 2, Event::Claim { term: 1 }), 1);
        c.accept(
            &packet(
                "a",
                3,
                Event::Switch {
                    term: 1,
                    target: "b".into(),
                    x: 20,
                    y: 20,
                },
            ),
            2,
        );
        let bad = packet(
            "b",
            2,
            Event::Input {
                term: 1,
                target: "b".into(),
                kind: InputKind::Key {
                    keycode: 38,
                    pressed: true,
                },
            },
        );
        assert!(!c.accept(&bad, 3));
        let mut restart = packet("a", 1, c.members["a"].presence.clone().into());
        restart.boot = "boot-2".into();
        assert!(c.accept(&restart, 4));
        assert!(c.lease.is_none());
        assert!(!c.accept(&packet("a", 5, Event::Claim { term: 2 }), 5));
    }
    impl From<Presence> for Event {
        fn from(presence: Presence) -> Self {
            Event::Presence { presence }
        }
    }
    #[test]
    fn unrelated_motion_reordering_does_not_drop_a_key_release() {
        let mut c = Control::default();
        join(&mut c, "a", true);
        join(&mut c, "b", true);
        c.accept(&packet("a", 2, Event::Claim { term: 1 }), 1);
        c.accept(
            &packet(
                "a",
                3,
                Event::Switch {
                    term: 1,
                    target: "b".into(),
                    x: 20,
                    y: 20,
                },
            ),
            2,
        );
        for (seq, kind) in [
            (
                4,
                InputKind::Key {
                    keycode: 38,
                    pressed: true,
                },
            ),
            (6, InputKind::MouseMove { x: 40, y: 40 }),
            (
                5,
                InputKind::Key {
                    keycode: 38,
                    pressed: false,
                },
            ),
        ] {
            assert!(c.accept(
                &packet(
                    "a",
                    seq,
                    Event::Input {
                        term: 1,
                        target: "b".into(),
                        kind
                    }
                ),
                3
            ));
        }
    }
    #[test]
    fn peer_encryption_hides_events_and_detects_modification() {
        let key = B64.encode([7; 32]);
        let wrong = B64.encode([8; 32]);
        let p = packet("private-machine", 1, Event::Claim { term: 1 });
        let wire = encrypt(&p, &key).unwrap();
        assert!(!wire.contains("private-machine"));
        assert!(!wire.contains("claim"));
        assert_eq!(decrypt(&wire, &key).unwrap().origin, p.origin);
        assert!(decrypt(&wire, &wrong).is_err());
        let mut env: Envelope = serde_json::from_str(&wire).unwrap();
        let mut bytes = B64.decode(&env.ciphertext).unwrap();
        bytes[0] ^= 1;
        env.ciphertext = B64.encode(bytes);
        assert!(decrypt(&serde_json::to_string(&env).unwrap(), &key).is_err());
    }
}
