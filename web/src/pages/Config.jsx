import { motion } from 'framer-motion'
import { GripVertical, MousePointer2, Save, Settings2 } from 'lucide-react'
import { useCallback, useEffect, useMemo, useState } from 'react'
import PageHeader from '../components/PageHeader'
import {
  fetchTopology,
  nodeLabel,
  saveTopology,
} from '../api'

const DIRS = ['left', 'right', 'up', 'down']
const DIR_LABEL = { left: '←', right: '→', up: '↑', down: '↓' }

function scaleLayout(nodes) {
  const entries = Object.entries(nodes || {})
  if (!entries.length) return { nodes: {}, scale: 0.2, width: 400, height: 200 }
  let maxX = 0
  let maxY = 0
  for (const [, n] of entries) {
    maxX = Math.max(maxX, n.x + n.width)
    maxY = Math.max(maxY, n.y + n.height)
  }
  const scale = Math.min(520 / Math.max(maxX, 1), 280 / Math.max(maxY, 1), 0.35)
  return { nodes, scale, width: maxX * scale + 40, height: maxY * scale + 40 }
}

export default function Config() {
  const [topology, setTopology] = useState(null)
  const [token, setToken] = useState(() => localStorage.getItem('poolsync_token') || '')
  const [error, setError] = useState(null)
  const [saved, setSaved] = useState(false)
  const [dragId, setDragId] = useState(null)

  const load = useCallback(async () => {
    try {
      const json = await fetchTopology()
      setTopology(json)
      setError(null)
    } catch (err) {
      setError(err.message || 'Erreur chargement')
    }
  }, [])

  useEffect(() => {
    load()
  }, [load])

  const layout = useMemo(() => scaleLayout(topology?.nodes), [topology])

  const nodeIds = useMemo(
    () => Object.keys(topology?.nodes || {}).sort((a, b) => {
      const na = topology.nodes[a]
      const nb = topology.nodes[b]
      return na.x - nb.x || na.y - nb.y
    }),
    [topology],
  )

  const updateNode = (id, patch) => {
    setTopology((prev) => ({
      ...prev,
      nodes: {
        ...prev.nodes,
        [id]: { ...prev.nodes[id], ...patch },
      },
    }))
    setSaved(false)
  }

  const setNeighbor = (id, dir, value) => {
    const node = topology.nodes[id]
    const neighbors = { ...node.neighbors }
    if (value) neighbors[dir] = value
    else delete neighbors[dir]
    updateNode(id, { neighbors })
  }

  const onDrag = (id, e) => {
    if (!dragId) return
    const scale = layout.scale
    const rect = e.currentTarget.parentElement.getBoundingClientRect()
    const x = Math.round((e.clientX - rect.left - 20) / scale)
    const y = Math.round((e.clientY - rect.top - 20) / scale)
    updateNode(id, { x: Math.max(0, x), y: Math.max(0, y) })
  }

  const handleSave = async () => {
    if (!token.trim()) {
      setError('Token requis pour enregistrer')
      return
    }
    localStorage.setItem('poolsync_token', token.trim())
    try {
      await saveTopology(topology, token.trim())
      setSaved(true)
      setError(null)
    } catch (err) {
      setError(err.message || 'Échec enregistrement')
    }
  }

  if (!topology) {
    return (
      <div className="mx-auto max-w-6xl p-8 text-slate-500">
        Chargement de la topologie…
      </div>
    )
  }

  return (
    <div className="mesh-bg mx-auto w-full max-w-6xl p-6 md:p-8">
      <PageHeader
        icon={Settings2}
        title="Configuration KVM"
        subtitle="Mosaïque d'écrans — voisins, position, activation clavier/souris par nœud"
      />

      <div className="mb-4 flex flex-wrap items-end gap-3">
        <label className="flex flex-col gap-1 text-sm">
          <span className="font-semibold text-slate-600">Token hub</span>
          <input
            type="password"
            value={token}
            onChange={(e) => setToken(e.target.value)}
            className="rounded-lg border border-slate-200 px-3 py-2 font-mono text-sm"
            placeholder="POOLSYNC_TOKEN"
          />
        </label>
        <button
          type="button"
          onClick={handleSave}
          className="inline-flex items-center gap-2 rounded-lg bg-indigo-600 px-4 py-2 text-sm font-semibold text-white shadow-sm hover:bg-indigo-700"
        >
          <Save size={16} />
          Enregistrer
        </button>
        {saved && <span className="text-sm text-emerald-600">Topologie envoyée aux agents</span>}
        {error && <span className="text-sm text-red-600">{error}</span>}
      </div>

      <div className="mb-6 rounded-xl border border-slate-200 bg-white p-4 shadow-sm">
        <div className="mb-3 flex items-center gap-2 text-sm font-semibold text-slate-700">
          <MousePointer2 size={16} className="text-indigo-600" />
          Mosaïque (glisser pour repositionner)
        </div>
        <div
          className="relative overflow-auto rounded-lg border border-dashed border-slate-200 bg-slate-50"
          style={{ minHeight: layout.height + 20 }}
        >
          <div className="relative" style={{ width: layout.width, height: layout.height }}>
            {nodeIds.map((id) => {
              const n = topology.nodes[id]
              return (
                <motion.div
                  key={id}
                  drag
                  dragMomentum={false}
                  onDragStart={() => setDragId(id)}
                  onDrag={(e) => onDrag(id, e)}
                  onDragEnd={() => setDragId(null)}
                  className={`absolute cursor-grab rounded-lg border-2 px-2 py-2 text-xs shadow-md active:cursor-grabbing ${
                    n.kvm_enabled
                      ? 'border-indigo-300 bg-indigo-50 text-indigo-900'
                      : 'border-slate-300 bg-slate-100 text-slate-500'
                  }`}
                  style={{
                    left: n.x * layout.scale + 20,
                    top: n.y * layout.scale + 20,
                    width: n.width * layout.scale,
                    height: n.height * layout.scale,
                  }}
                >
                  <div className="flex items-center justify-between gap-1 font-bold">
                    <span className="flex items-center gap-1 truncate">
                      <GripVertical size={12} className="opacity-40" />
                      {nodeLabel(id)}
                    </span>
                    <span className="font-mono opacity-60">{n.width}×{n.height}</span>
                  </div>
                  {!n.kvm_enabled && (
                    <div className="mt-1 text-[10px] font-semibold uppercase text-slate-400">
                      KVM off
                    </div>
                  )}
                </motion.div>
              )
            })}
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
        {nodeIds.map((id) => {
          const n = topology.nodes[id]
          return (
            <div key={id} className="rounded-xl border border-slate-200 bg-white p-4 shadow-sm">
              <div className="mb-3 flex items-center justify-between">
                <h3 className="font-bold text-slate-800">{nodeLabel(id)}</h3>
                <label className="flex items-center gap-2 text-sm">
                  <input
                    type="checkbox"
                    checked={n.kvm_enabled}
                    onChange={(e) => updateNode(id, { kvm_enabled: e.target.checked })}
                  />
                  KVM actif
                </label>
              </div>
              <div className="mb-3 grid grid-cols-2 gap-2 text-sm">
                <label className="flex flex-col gap-1">
                  Largeur
                  <input
                    type="number"
                    value={n.width}
                    onChange={(e) => updateNode(id, { width: Number(e.target.value) })}
                    className="rounded border border-slate-200 px-2 py-1"
                  />
                </label>
                <label className="flex flex-col gap-1">
                  Hauteur
                  <input
                    type="number"
                    value={n.height}
                    onChange={(e) => updateNode(id, { height: Number(e.target.value) })}
                    className="rounded border border-slate-200 px-2 py-1"
                  />
                </label>
              </div>
              <div className="space-y-2">
                {DIRS.map((dir) => (
                  <div key={dir} className="flex items-center gap-2 text-sm">
                    <span className="w-6 text-center font-mono text-slate-400">{DIR_LABEL[dir]}</span>
                    <select
                      value={n.neighbors?.[dir] || ''}
                      onChange={(e) => setNeighbor(id, dir, e.target.value || null)}
                      className="flex-1 rounded border border-slate-200 px-2 py-1"
                    >
                      <option value="">—</option>
                      {nodeIds
                        .filter((other) => other !== id)
                        .map((other) => (
                          <option key={other} value={other}>
                            {nodeLabel(other)}
                          </option>
                        ))}
                    </select>
                  </div>
                ))}
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}
