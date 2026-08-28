//! Ordre total du presse-papiers sur le mesh (horloge de Lamport).
//!
//! Le mesh est multi-saut et le hub relaie en parallèle des liens directs :
//! un même contenu arrive plusieurs fois, et un contenu *ancien* peut arriver
//! après un contenu récent. Historiquement on arbitrait avec des fenêtres
//! temporelles (« priorité copie locale 4 s », « grâce doublon image »), qui se
//! recouvrent différemment selon la latence — donc non déterministes dès qu'il
//! y a plus de deux nœuds.
//!
//! Ici chaque copie porte `(origin, seq)`. `seq` est une horloge de Lamport
//! amorcée sur l'heure mur, donc croissante même après un redémarrage. Un nœud
//! applique un message seulement s'il est strictement postérieur au dernier
//! appliqué, l'égalité étant tranchée par le nom du nœud. Conséquences :
//! - une copie locale prend une horloge supérieure à tout ce qui a été vu :
//!   elle gagne contre un message plus ancien encore en vol ;
//! - le doublon hub + pair a le même `(origin, seq)` : rejeté, sans minuterie ;
//! - le retour d'écho de notre propre message est rejeté pour la même raison.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Debug)]
pub struct ClipOrder {
    local_node: String,
    clock: AtomicU64,
    /// Dernier presse-papiers appliqué ou émis : (seq, origin).
    last: Mutex<(u64, String)>,
}

impl ClipOrder {
    pub fn new(local_node: impl Into<String>) -> Self {
        Self {
            local_node: local_node.into(),
            clock: AtomicU64::new(now_ms()),
            last: Mutex::new((0, String::new())),
        }
    }

    /// Horloge à attacher à une copie faite sur cette machine.
    pub fn next_local_seq(&self) -> u64 {
        let seq = self.bump(now_ms());
        if let Ok(mut last) = self.last.lock() {
            *last = (seq, self.local_node.clone());
        }
        seq
    }

    /// `true` si ce message doit être appliqué, et enregistre-le comme dernier.
    ///
    /// `seq == 0` ou `origin` vide = agent d'une version antérieure : on
    /// applique sans contrôle d'ordre pour rester compatible dans un pool mixte.
    pub fn accept_incoming(&self, origin: &str, seq: u64) -> bool {
        if seq == 0 || origin.is_empty() {
            return true;
        }
        // Lamport : notre horloge dépasse tout ce que l'on a observé, donc la
        // prochaine copie locale gagnera contre ce message.
        self.bump(seq);
        if origin == self.local_node {
            return false; // notre propre message revenu par un pair ou le hub
        }
        let Ok(mut last) = self.last.lock() else {
            return true;
        };
        if !is_newer((seq, origin), (last.0, last.1.as_str())) {
            return false;
        }
        *last = (seq, origin.to_string());
        true
    }

    fn bump(&self, observed: u64) -> u64 {
        let mut current = self.clock.load(Ordering::SeqCst);
        loop {
            let next = current.max(observed) + 1;
            match self.clock.compare_exchange_weak(
                current,
                next,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => return next,
                Err(actual) => current = actual,
            }
        }
    }
}

/// Ordre total : horloge d'abord, nom du nœud pour départager les égalités.
fn is_newer(candidate: (u64, &str), last: (u64, &str)) -> bool {
    (candidate.0, candidate.1) > (last.0, last.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_copy_wins_over_an_older_message_still_in_flight() {
        let order = ClipOrder::new("asus");
        // Un pair a copié il y a un instant, le message met du temps à arriver.
        let stale = order.next_local_seq() - 1;
        order.next_local_seq(); // l'utilisateur copie ici, maintenant
        assert!(!order.accept_incoming("gbs-p2", stale));
    }

    #[test]
    fn the_same_message_via_hub_and_peer_is_applied_once() {
        let order = ClipOrder::new("asus");
        assert!(order.accept_incoming("gbs-p2", 1_000));
        assert!(!order.accept_incoming("gbs-p2", 1_000));
    }

    #[test]
    fn our_own_message_coming_back_through_the_mesh_is_ignored() {
        let order = ClipOrder::new("asus");
        let seq = order.next_local_seq();
        assert!(!order.accept_incoming("asus", seq));
    }

    #[test]
    fn a_newer_remote_copy_is_applied_even_right_after_a_local_one() {
        let order = ClipOrder::new("asus");
        let local = order.next_local_seq();
        assert!(order.accept_incoming("gbs-p2", local + 1));
    }

    #[test]
    fn observing_a_peer_clock_lifts_our_own_so_the_next_local_copy_wins() {
        let order = ClipOrder::new("asus");
        order.accept_incoming("gbs-p2", 9_000_000_000_000);
        assert!(order.next_local_seq() > 9_000_000_000_000);
    }

    #[test]
    fn equal_clocks_are_broken_by_node_name_the_same_way_everywhere() {
        assert!(is_newer((5, "zzz"), (5, "aaa")));
        assert!(!is_newer((5, "aaa"), (5, "zzz")));
        assert!(!is_newer((5, "aaa"), (5, "aaa")));
    }

    #[test]
    fn legacy_messages_without_a_clock_are_still_applied() {
        let order = ClipOrder::new("asus");
        order.accept_incoming("gbs-p2", 1_000);
        assert!(order.accept_incoming("", 0));
        assert!(order.accept_incoming("old-agent", 0));
    }

    #[test]
    fn local_sequence_is_strictly_increasing() {
        let order = ClipOrder::new("asus");
        let a = order.next_local_seq();
        let b = order.next_local_seq();
        assert!(b > a);
    }
}
