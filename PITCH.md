# PoolSync — One desk, many machines

## One-liner

**PoolSync** does what **Barrier** does — one keyboard, one mouse, one shared clipboard across several screens — but **without a fixed master**: the machine you are using *becomes* the master, wherever you sit in the room.

---

## Same use case as Barrier

Barrier (or Synergy) spreads **one desktop across several computers** on a network:

- **Server or desktop** — each machine runs a graphical session (XFCE, etc.).
- Install an agent on **every** machine in the pool.
- Define **topology**: who is left, right, above, below — like monitors on one large desk.
- Move the mouse **off the edge** of a screen onto the neighbour; keyboard follows; **clipboard is shared** (`clipboardSharing`, on by default in Barrier).

Pools of **3, 5, 6, 7+ machines** are supported.

### Clipboard with Barrier (important nuance)

Barrier **does share the clipboard** across the pool — it is officially documented. In practice:

| | Barrier |
|---|---------|
| **Text** | Yes — primary use case |
| **Images** | Theoretically supported, but **often broken on Linux** (screenshots, PNG); slow or unstable for large images |
| **Reliability** | Can break after reboot, VPN changes, or conflict with clipman / RDP |

PoolSync uses the same idea with a **text + image** clipboard that is more robust on XFCE (GTK, webmail paste).

---

## Concrete scenario: six computers in one room

Six machines on a long desk, left to right.

You sit **on the far right**, on your daily workstation.

On the **far left**, a machine you are still **setting up**.

### With Barrier

- Pick **one fixed master** and **slaves**.
- Keyboard and mouse only drive other machines **from the master**.
- To configure the left machine while working on the right, you usually **move to the master** — or accept a rigid layout.

**Result:** you move physically, or live with a fixed master.

### With PoolSync

- **No fixed master** — *whoever is in use becomes master*.
- You stay **on the right**; your machine is master while you move the mouse there.
- Slide the mouse left, screen by screen, to the setup machine — **without changing chair**.
- Install agents where you want; wire topology once; control the **whole pool from where you are**.

**Result:** one keyboard/mouse, one clipboard, **freedom of position** — master follows the user.

---

## Problems (beyond Barrier)

Even with Barrier, day to day:

- clipboard **text** OK in theory, **images** unreliable on Linux;
- clipboard breaks (VPN, RDP, reboot, clipman conflicts);
- instability after network / VPN changes;
- no dashboard — who is online, who is master;
- fixed master while work moves across machines.

---

## Solution

**PoolSync** = **lightweight hub** + **agent per machine** + **GUI** (systray, web dashboard).

| Feature | What it does |
|---------|----------------|
| **Multi-screen KVM** | Like Barrier: edge switching, mosaic topology |
| **Dynamic master** | Machine under the mouse becomes master — no fixed server/slave |
| **Shared clipboard** | Text and images in real time |
| **Web dashboard** | Topology, node status, who is master |
| **Systray** | Status from each desktop (config + logs planned) |
| **VPN resilience** | Auto-reconnect when the VPN comes back |

---

## Who is it for?

Anyone with **several Linux workstations** (laptop, desktop, graphically logged-in server) on a private network (WireGuard VPN, LAN, etc.).

Examples:

- **Tech bench** — 5–7 machines, one keyboard/mouse.
- **Laptop fleet** — three machines, hub on a small VPS.
- **Mixed server + desktop** — any X11 graphical session can join the pool.

---

## PoolSync vs Barrier

| | Barrier | PoolSync |
|---|---------|----------|
| **Use case** | KVM + shared clipboard | **Same** |
| **Master** | **Fixed** — one server, others slaves | **Dynamic** — machine in use |
| **Where you work** | Often at the master | **Any pool machine** |
| **Topology** | Config file | Web mosaic + API |
| **Clipboard** | Shared (text ✅; images flaky on Linux) | Text + robust images (GTK) |
| **VPN / reconnect** | Fragile | Watchdog + hub retry |
| **Visibility** | No dashboard | Web + systray |
| **Stack** | Legacy C++ | Rust, systemd |

---

## Architecture

```
[Node 1] ←→ [Hub :9470] ←→ [Node 2]
                ↑
           [Node 3 … N]
```

Physical layout example:

```
[Setup] — [Dev] — [Test] — [Prod] — [Mail] — [You ▶]
  left                                              right
```

1. Each agent connects to the hub (WebSocket / VPN).
2. You are on the right → your node is master.
3. Mouse left → you drive the setup machine, then return without standing up.
4. Paste on any node → propagated to the whole pool.

---

## Stack

- **Rust** — `poolsync-agent`, `poolsync-hub`, `poolsync-core`
- **X11** — Barrier-style grab, clipboard, injection
- **Web** — React dashboard (topology, live status)
- **Deploy** — systemd user + VPN watchdog

**Repo:** https://github.com/xavdp-pro/poolsync

---

## Elevator pitch (30 seconds)

> *"PoolSync is Barrier rebuilt for real life. Same idea: several computers, one keyboard, one mouse, shared clipboard — server or desktop, graphical UI. With Barrier you need a fixed master and often sit at the server machine. With PoolSync, **whoever you use becomes master**. Six PCs on a desk, you're on the right working, you slide the mouse to the setup machine on the left — without moving your chair. Lightweight hub on your VPN, no cloud."*

---

## Short share text (chat / social)

```
PoolSync — Barrier without a fixed master

Barrier: one master server + slaves → keyboard/mouse from the master only
PoolSync: no fixed master → the machine you use becomes master

Example: 6 PCs on a desk, you're on the right, setup PC on the left
→ PoolSync: stay seated, mouse left, you control it — master is you, now

+ shared clipboard (text + images on Linux)
+ web dashboard + VPN reconnect

https://github.com/xavdp-pro/poolsync
```
