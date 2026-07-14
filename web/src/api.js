const POOL_ORDER = ['asus', 'acer', 'inspiron', 'gbs-p2']

const POOL_META = {
  asus: { label: 'Asus', vpn: '10.24.42.6', role: 'Portable principal' },
  acer: { label: 'Acer', vpn: '10.24.42.4', role: 'Portable — client RDP gbs-p2' },
  inspiron: { label: 'Inspiron', vpn: '10.24.42.5', role: 'Portable' },
  'gbs-p2': { label: 'gbs-p2', vpn: '10.24.42.18', role: 'Session RDP — clipboard seul' },
}

export { POOL_ORDER, POOL_META }

export async function fetchStatus() {
  const res = await fetch('/api/status', { cache: 'no-store' })
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  return res.json()
}

export async function fetchTopology() {
  const res = await fetch('/api/topology', { cache: 'no-store' })
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
  return res.json()
}

export async function saveTopology(topology, token) {
  const res = await fetch(`/api/topology?token=${encodeURIComponent(token)}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(topology),
  })
  if (!res.ok) throw new Error(`HTTP ${res.status}`)
}

export function formatTs(ts) {
  if (!ts) return '—'
  return new Date(ts * 1000).toLocaleString('fr-FR', {
    day: '2-digit',
    month: 'short',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

export function modeLabel(mode) {
  if (mode === 'clipboard_only') return 'Presse-papiers'
  if (mode === 'full') return 'Complet (clip + KVM)'
  return mode
}

export function shortHash(hash) {
  if (!hash) return '—'
  return `${hash.slice(0, 8)}…${hash.slice(-6)}`
}
