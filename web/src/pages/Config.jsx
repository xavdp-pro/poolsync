import { motion } from 'framer-motion'
import { LayoutGrid, Link2, MousePointer2, Save, Settings2 } from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import PageHeader from '../components/PageHeader'
import {
  fetchStatus,
  fetchTopology,
  nodeLabel,
  saveTopology,
} from '../api'
import NodeBadges, { LastClip } from '../components/NodeBadges'
import MonitorMap from '../components/MonitorMap'
import {
  CANVAS_PAD,
  connectionLines,
  inferNeighbors,
  isParked,
  nodeRect,
  scaleLayout,
  snapPosition,
  SNAP_GRID_PX,
} from '../topologyLayout'

const DIRS = ['left', 'right', 'up', 'down']
const DIR_LABEL = { left: '←', right: '→', up: '↑', down: '↓' }

export default function Config() {
  const [topology, setTopology] = useState(null)
  const [token, setToken] = useState(() => localStorage.getItem('poolsync_token') || '')
  const [error, setError] = useState(null)
  const [saved, setSaved] = useState(false)
  const [dragId, setDragId] = useState(null)
  // État vivant des nœuds (en ligne, synchro, pause, maître, dernière copie).
  // Rafraîchi en continu : la mosaïque doit refléter le pool tel qu'il est,
  // pas seulement la géométrie enregistrée.
  const [statusByNode, setStatusByNode] = useState({})
  const dragStart = useRef({})
  const canvasRef = useRef(null)

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

  useEffect(() => {
    let alive = true
    const refresh = async () => {
      try {
        const json = await fetchStatus()
        if (!alive) return
        const byNode = {}
        for (const n of json.nodes || []) byNode[n.name] = n
        setStatusByNode(byNode)
      } catch {
        // Le hub peut être momentanément injoignable : on garde le dernier
        // état connu plutôt que de faire clignoter toute la mosaïque.
      }
    }
    refresh()
    const timer = setInterval(refresh, 3000)
    return () => {
      alive = false
      clearInterval(timer)
    }
  }, [])

  const layout = useMemo(() => scaleLayout(topology?.nodes), [topology])

  const lines = useMemo(
    () => connectionLines(topology?.nodes, layout.scale),
    [topology, layout.scale],
  )

  const allIds = useMemo(
    () => Object.keys(topology?.nodes || {}).sort((a, b) => {
      const na = topology.nodes[a]
      const nb = topology.nodes[b]
      return na.x - nb.x || na.y - nb.y
    }),
    [topology],
  )
  // Les machines sans KVM sont garées hors mosaïque par le hub : on les liste
  // à part plutôt que de les dessiner à 100 000 px, ce qui écrasait l'échelle.
  const nodeIds = useMemo(
    () => allIds.filter((id) => !isParked(topology?.nodes?.[id])),
    [allIds, topology],
  )
  const parkedIds = useMemo(
    () => allIds.filter((id) => isParked(topology?.nodes?.[id])),
    [allIds, topology],
  )

  const applyTopology = (nodes) => {
    setTopology((prev) => ({ ...prev, nodes }))
    setSaved(false)
  }

  const updateNode = (id, patch) => {
    applyTopology({
      ...topology.nodes,
      [id]: { ...topology.nodes[id], ...patch },
    })
  }

  const setNeighbor = (id, dir, value) => {
    const node = topology.nodes[id]
    const neighbors = { ...node.neighbors }
    if (value) neighbors[dir] = value
    else delete neighbors[dir]
    updateNode(id, { neighbors })
  }

  const recalcNeighbors = () => {
    const next = inferNeighbors(topology)
    applyTopology(next.nodes)
  }

  const onDragEnd = (id, _e, info) => {
    setDragId(null)
    const start = dragStart.current[id] || topology.nodes[id]
    const [sx, sy] = snapPosition(
      start.x + info.offset.x / layout.scale,
      start.y + info.offset.y / layout.scale,
    )
    const patched = {
      ...topology.nodes,
      [id]: { ...topology.nodes[id], x: Math.max(0, sx), y: Math.max(0, sy) },
    }
    const next = inferNeighbors({ nodes: patched })
    applyTopology(next.nodes)
  }

  const handleSave = async () => {
    if (!token.trim()) {
      setError('Token requis pour enregistrer')
      return
    }
    localStorage.setItem('poolsync_token', token.trim())
    const toSave = inferNeighbors(topology)
    try {
      await saveTopology(toSave, token.trim())
      setTopology(toSave)
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
        subtitle="Glissez les écrans comme dans Barrier — les voisins se recalculent automatiquement"
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
          onClick={recalcNeighbors}
          className="inline-flex items-center gap-2 rounded-lg border border-slate-200 bg-white px-4 py-2 text-sm font-semibold text-slate-700 shadow-sm hover:bg-slate-50"
        >
          <Link2 size={16} />
          Recalculer voisins
        </button>
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
        <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
          <div className="flex items-center gap-2 text-sm font-semibold text-slate-700">
            <MousePointer2 size={16} className="text-indigo-600" />
            Mosaïque d&apos;écrans
          </div>
          <span className="text-xs text-slate-400">
            Grille {SNAP_GRID_PX}px · alignez les bords pour lier les machines
          </span>
        </div>
        <div
          ref={canvasRef}
          className="relative overflow-auto rounded-lg border border-dashed border-slate-300 bg-[repeating-linear-gradient(0deg,transparent,transparent_19px,#e2e8f0_19px,#e2e8f0_20px),repeating-linear-gradient(90deg,transparent,transparent_19px,#e2e8f0_19px,#e2e8f0_20px)] bg-slate-50"
          style={{ minHeight: layout.height + 20 }}
        >
          <div className="relative" style={{ width: layout.width, height: layout.height }}>
            <svg
              className="pointer-events-none absolute inset-0"
              width={layout.width}
              height={layout.height}
              aria-hidden
            >
              {lines.map((ln) => (
                <line
                  key={`${ln.a}-${ln.b}`}
                  x1={ln.x1}
                  y1={ln.y1}
                  x2={ln.x2}
                  y2={ln.y2}
                  stroke="#6366f1"
                  strokeWidth={2}
                  strokeDasharray="6 4"
                  opacity={0.7}
                />
              ))}
            </svg>

            {nodeIds.map((id) => {
              const n = topology.nodes[id]
              const rect = nodeRect(n, layout.scale)
              const active = dragId === id
              const status = statusByNode[id]
              const offline = !status || !status.online
              const deaf = status?.online && !status.clipboard_sync
              return (
                <motion.div
                  key={id}
                  drag
                  dragMomentum={false}
                  dragElastic={0}
                  onDragStart={() => {
                    setDragId(id)
                    dragStart.current[id] = { x: n.x, y: n.y }
                  }}
                  onDragEnd={(e, info) => onDragEnd(id, e, info)}
                  className={`absolute flex cursor-grab flex-col items-center justify-center overflow-hidden rounded-md border-2 text-center shadow-lg active:cursor-grabbing ${
                    offline
                      ? 'border-slate-300 border-dashed bg-slate-100/70 text-slate-400'
                      : deaf
                        ? 'border-red-400 bg-red-50/90 text-red-900'
                        : n.kvm_enabled
                          ? 'border-indigo-400 bg-indigo-100/90 text-indigo-950'
                          : 'border-slate-400 bg-slate-200/90 text-slate-600'
                  } ${active ? 'z-20 ring-2 ring-indigo-500' : 'z-10'}`}
                  style={{
                    left: rect.left,
                    top: rect.top,
                    width: Math.max(rect.width, 72),
                    height: Math.max(rect.height, 48),
                  }}
                  title={`${id} — ${n.width}×${n.height}`}
                >
                  <NodeBadges status={status} />
                  <LayoutGrid size={14} className="mb-0.5 opacity-40" />
                  <span className="px-1 text-sm font-bold leading-tight">{nodeLabel(id)}</span>
                  <span className="font-mono text-[10px] opacity-60">{n.width}×{n.height}</span>
                  <MonitorMap
                    monitors={status?.monitors}
                    poolMonitor={{ x: n.monitor_x, y: n.monitor_y }}
                  />
                  <LastClip clip={status?.last_clip} />
                  {!n.kvm_enabled && (
                    <span className="mt-0.5 text-[9px] font-semibold uppercase tracking-wide text-slate-500">
                      clip only
                    </span>
                  )}
                </motion.div>
              )
            })}
          </div>
        </div>
        <p className="mt-2 text-xs text-slate-500">
          Position ({CANVAS_PAD}px marge) :{' '}
          {nodeIds.map((id) => {
            const n = topology.nodes[id]
            return `${nodeLabel(id)} (${n.x}, ${n.y})`
          }).join(' · ')}
        </p>

        {parkedIds.length > 0 && (
          <div className="mt-4 border-t border-slate-200 pt-3">
            <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-slate-500">
              Presse-papiers seul — hors mosaïque KVM
            </p>
            <div className="flex flex-wrap gap-2">
              {parkedIds.map((id) => {
                const status = statusByNode[id]
                return (
                  <div
                    key={id}
                    className={`relative flex min-w-[7rem] flex-col rounded-md border px-2 py-1.5 text-xs ${
                      !status || !status.online
                        ? 'border-dashed border-slate-300 bg-slate-50 text-slate-400'
                        : status.clipboard_sync
                          ? 'border-slate-300 bg-white text-slate-700'
                          : 'border-red-300 bg-red-50 text-red-900'
                    }`}
                    title={`${id} — ${topology.nodes[id].width}×${topology.nodes[id].height}`}
                  >
                    <NodeBadges status={status} />
                    <span className="font-semibold">{nodeLabel(id)}</span>
                    <span className="font-mono text-[10px] opacity-60">
                      {topology.nodes[id].width}×{topology.nodes[id].height}
                    </span>
                    <MonitorMap
                      monitors={status?.monitors}
                      poolMonitor={{
                        x: topology.nodes[id].monitor_x,
                        y: topology.nodes[id].monitor_y,
                      }}
                    />
                    <LastClip clip={status?.last_clip} />
                  </div>
                )
              })}
            </div>
          </div>
        )}
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
