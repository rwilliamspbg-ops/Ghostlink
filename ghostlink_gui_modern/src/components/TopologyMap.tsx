import React, { useMemo, useState, useCallback, useEffect } from 'react';
import { Server, Cpu, HardDrive, Zap, X } from 'lucide-react';
import { ClusterTopology, ClusterTopologyNode, ClusterTopologyEdge } from '../api';

// ─── Constants ───────────────────────────────────────────────────────────────

const SVG_SIZE = 480;
const CENTER = SVG_SIZE / 2;
const COORDINATOR_RADIUS = 48;
const PEER_RADIUS_MIN = 28;
const PEER_RADIUS_MAX = 52;
const CIRCLE_RADIUS = 180;
const PULSE_DURATION = 2.5;

const ROLE_COLORS: Record<string, { fill: string; stroke: string; glow: string }> = {
  coordinator: { fill: '#1e3a5f', stroke: '#3b82f6', glow: 'rgba(59, 130, 246, 0.4)' },
  contributor: { fill: '#1a3a2a', stroke: '#22c55e', glow: 'rgba(34, 197, 94, 0.4)' },
  unused: { fill: '#3a2a1a', stroke: '#f59e0b', glow: 'rgba(245, 158, 11, 0.4)' },
};

// ─── Helpers ─────────────────────────────────────────────────────────────────

function getRole(node: ClusterTopologyNode): string {
  if (node.role === 'coordinator') return 'coordinator';
  if (node.role === 'contributor' && !node.excluded_reason) return 'contributor';
  return 'unused';
}

function getNodeRadius(node: ClusterTopologyNode, allNodes: ClusterTopologyNode[]): number {
  const role = getRole(node);
  if (role === 'coordinator') return COORDINATOR_RADIUS;

  const vramValues = allNodes.map((n) => n.vram_gb);
  const maxVram = Math.max(...vramValues, 1);
  const minVram = Math.min(...vramValues, 0);
  const range = maxVram - minVram || 1;
  const normalized = (node.vram_gb - minVram) / range;
  return PEER_RADIUS_MIN + normalized * (PEER_RADIUS_MAX - PEER_RADIUS_MIN);
}

function getEdgeStrokeWidth(latencyUs: number): number {
  // Map latency to stroke width: 0µs → 1px, 500µs+ → 6px
  const clamped = Math.min(latencyUs, 500);
  return 1 + (clamped / 500) * 5;
}

function getEdgeColor(latencyUs: number): string {
  if (latencyUs < 100) return '#22c55e';
  if (latencyUs < 300) return '#f59e0b';
  return '#ef4444';
}

function polarToCartesian(cx: number, cy: number, r: number, angleDeg: number) {
  const angleRad = ((angleDeg - 90) * Math.PI) / 180;
  return {
    x: cx + r * Math.cos(angleRad),
    y: cy + r * Math.sin(angleRad),
  };
}

// ─── Component ───────────────────────────────────────────────────────────────

export interface TopologyMapProps {
  topology: ClusterTopology;
  className?: string;
}

export const TopologyMap: React.FC<TopologyMapProps> = ({ topology, className = '' }) => {
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);

  const { nodes, edges } = topology;

  const coordinator = useMemo(
    () => nodes.find((n) => getRole(n) === 'coordinator') ?? nodes[0],
    [nodes],
  );

  const peers = useMemo(() => {
    if (!coordinator) return nodes;
    return nodes.filter((n) => n.id !== coordinator.id);
  }, [nodes, coordinator]);

  const nodePositionMap = useMemo(() => {
    const map = new Map<string, { x: number; y: number; radius: number }>();
    if (!coordinator) return map;

    map.set(coordinator.id, {
      x: CENTER,
      y: CENTER,
      radius: getNodeRadius(coordinator, nodes),
    });

    const peerCount = peers.length;
    peers.forEach((peer, i) => {
      const angle = (360 / peerCount) * i;
      const pos = polarToCartesian(CENTER, CENTER, CIRCLE_RADIUS, angle);
      map.set(peer.id, {
        x: pos.x,
        y: pos.y,
        radius: getNodeRadius(peer, nodes),
      });
    });

    return map;
  }, [nodes, coordinator, peers]);

  const selectedNode = useMemo(
    () => (selectedNodeId ? nodes.find((n) => n.id === selectedNodeId) ?? null : null),
    [selectedNodeId, nodes],
  );

  const handleNodeClick = useCallback((nodeId: string) => {
    setSelectedNodeId((prev) => (prev === nodeId ? null : nodeId));
  }, []);

  const handleClose = useCallback(() => {
    setSelectedNodeId(null);
  }, []);

  useEffect(() => {
    if (!selectedNodeId) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setSelectedNodeId(null);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [selectedNodeId]);

  if (!topology || !nodes.length) {
    return (
      <div
        className={`bg-slate-900/80 border border-slate-800 rounded-3xl flex items-center justify-center text-slate-500 text-sm ${className}`}
        style={{ minHeight: 320 }}
      >
        No topology data available
      </div>
    );
  }

  return (
    <div
      className={`bg-slate-900/80 border border-slate-800 rounded-3xl p-4 relative ${className}`}
    >
      <svg
        viewBox={`0 0 ${SVG_SIZE} ${SVG_SIZE}`}
        className="w-full h-auto"
        style={{ maxWidth: SVG_SIZE }}
        role="img"
        aria-label="Cluster topology map"
      >
        <defs>
          <filter id="node-glow" x="-50%" y="-50%" width="200%" height="200%">
            <feGaussianBlur stdDeviation="4" result="blur" />
            <feMerge>
              <feMergeNode in="blur" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
          <radialGradient id="coordinator-gradient" cx="50%" cy="50%" r="50%">
            <stop offset="0%" stopColor="#1e40af" />
            <stop offset="100%" stopColor="#1e3a5f" />
          </radialGradient>
        </defs>

        {/* Edges */}
        {edges.map((edge: ClusterTopologyEdge) => {
          const fromPos = nodePositionMap.get(edge.from);
          const toPos = nodePositionMap.get(edge.to);
          if (!fromPos || !toPos) return null;

          const strokeWidth = getEdgeStrokeWidth(edge.latency_us);
          const strokeColor = getEdgeColor(edge.latency_us);

          return (
            <g key={`${edge.from}-${edge.to}`}>
              <line
                x1={fromPos.x}
                y1={fromPos.y}
                x2={toPos.x}
                y2={toPos.y}
                stroke={strokeColor}
                strokeWidth={strokeWidth}
                strokeOpacity={0.6}
                strokeLinecap="round"
              />
              {/* Latency label at midpoint */}
              <text
                x={(fromPos.x + toPos.x) / 2}
                y={(fromPos.y + toPos.y) / 2 - 4}
                textAnchor="middle"
                fill={strokeColor}
                fontSize={10}
                fontFamily="monospace"
                opacity={0.9}
              >
                {edge.latency_us}µs
              </text>
            </g>
          );
        })}

        {/* Nodes */}
        {nodes.map((node) => {
          const pos = nodePositionMap.get(node.id);
          if (!pos) return null;

          const role = getRole(node);
          const colors = ROLE_COLORS[role];
          const isSelected = selectedNodeId === node.id;
          const isCoordinator = role === 'coordinator';

          const titleText = `${node.label} (${role}) — ${
            node.vram_gb > 0 ? `${node.vram_gb} GB VRAM` : `${node.system_memory_gb} GB System RAM`
          }. Click or press Enter to toggle details.`;

          return (
            <g
              key={node.id}
              onClick={() => handleNodeClick(node.id)}
              style={{ cursor: 'pointer' }}
              role="button"
              aria-label={`Node ${node.label} (${role})`}
              aria-expanded={isSelected}
              tabIndex={0}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  handleNodeClick(node.id);
                }
              }}
              className="focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-500 rounded-full"
            >
              <title>{titleText}</title>

              {/* Pulse ring for active contributors */}
              {role === 'contributor' && (
                <circle
                  cx={pos.x}
                  cy={pos.y}
                  r={pos.radius + 4}
                  fill="none"
                  stroke={colors.stroke}
                  strokeWidth={2}
                  opacity={0}
                >
                  <animate
                    attributeName="r"
                    from={String(pos.radius + 4)}
                    to={String(pos.radius + 20)}
                    dur={`${PULSE_DURATION}s`}
                    repeatCount="indefinite"
                  />
                  <animate
                    attributeName="opacity"
                    from="0.6"
                    to="0"
                    dur={`${PULSE_DURATION}s`}
                    repeatCount="indefinite"
                  />
                </circle>
              )}

              {/* Selection ring */}
              {isSelected && (
                <circle
                  cx={pos.x}
                  cy={pos.y}
                  r={pos.radius + 8}
                  fill="none"
                  stroke="#ffffff"
                  strokeWidth={2}
                  strokeDasharray="4 4"
                  opacity={0.8}
                />
              )}

              {/* Node circle */}
              <circle
                cx={pos.x}
                cy={pos.y}
                r={pos.radius}
                fill={isCoordinator ? 'url(#coordinator-gradient)' : colors.fill}
                stroke={colors.stroke}
                strokeWidth={isSelected ? 3 : 2}
                filter="url(#node-glow)"
              />

              {/* Node icon */}
              <g transform={`translate(${pos.x - 12}, ${pos.y - 24})`}>
                {isCoordinator ? (
                  <foreignObject x={0} y={0} width={24} height={24}>
                    <Server size={24} color={colors.stroke} />
                  </foreignObject>
                ) : (
                  <foreignObject x={0} y={0} width={24} height={24}>
                    <Cpu size={24} color={colors.stroke} />
                  </foreignObject>
                )}
              </g>

              {/* VRAM label */}
              <text
                x={pos.x}
                y={pos.y + 4}
                textAnchor="middle"
                fill="#e2e8f0"
                fontSize={11}
                fontWeight="bold"
                fontFamily="monospace"
              >
                {node.vram_gb > 0 ? `${node.vram_gb}GB` : 'CPU'}
              </text>

              {/* Node label */}
              <text
                x={pos.x}
                y={pos.y + pos.radius + 16}
                textAnchor="middle"
                fill="#94a3b8"
                fontSize={11}
                fontWeight="500"
              >
                {node.label}
              </text>
            </g>
          );
        })}
      </svg>

      {/* Detail Popover */}
      {selectedNode && (
        <div className="absolute top-4 right-4 w-72 bg-slate-950/95 border border-slate-700 rounded-2xl p-4 shadow-2xl z-10">
          <div className="flex items-center justify-between mb-3">
            <h3 className="text-sm font-bold text-slate-100 flex items-center gap-2">
              <Server size={16} className="text-blue-400" />
              {selectedNode.label}
            </h3>
            <button
              onClick={handleClose}
              className="text-slate-400 hover:text-slate-200 hover:bg-slate-800 p-1 rounded-lg transition focus-visible:ring-2 focus-visible:ring-blue-500 focus-visible:outline-none"
              aria-label="Close details"
              title="Close details (Esc)"
            >
              <X size={16} />
            </button>
          </div>

          <div className="space-y-2 text-xs">
            <div className="flex justify-between">
              <span className="text-slate-400">Role</span>
              <span className="text-slate-200 font-medium capitalize">
                {getRole(selectedNode)}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-slate-400">Status</span>
              <span className="text-slate-200 font-medium">{selectedNode.status}</span>
            </div>
            <div className="flex justify-between items-center">
              <span className="text-slate-400 flex items-center gap-1">
                <HardDrive size={12} /> VRAM
              </span>
              <span className="text-slate-200 font-medium font-mono">
                {selectedNode.vram_gb > 0 ? `${selectedNode.vram_gb} GB` : 'N/A'}
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-slate-400">System Memory</span>
              <span className="text-slate-200 font-medium font-mono">
                {selectedNode.system_memory_gb} GB
              </span>
            </div>
            <div className="flex justify-between items-center">
              <span className="text-slate-400 flex items-center gap-1">
                <Zap size={12} /> Latency
              </span>
              <span className="text-slate-200 font-medium font-mono">
                {selectedNode.latency_us} µs
              </span>
            </div>
            <div className="flex justify-between">
              <span className="text-slate-400">Throughput</span>
              <span className="text-slate-200 font-medium font-mono">
                {selectedNode.throughput_gbps} Gbps
              </span>
            </div>
            {selectedNode.ip_address && (
              <div className="flex justify-between">
                <span className="text-slate-400">Address</span>
                <span className="text-slate-200 font-medium font-mono">
                  {selectedNode.ip_address}
                  {selectedNode.rpc_port ? `:${selectedNode.rpc_port}` : ''}
                </span>
              </div>
            )}
            {selectedNode.excluded_reason && (
              <div className="mt-2 p-2 bg-amber-950/40 border border-amber-800/50 rounded-lg">
                <span className="text-amber-400 text-[11px]">{selectedNode.excluded_reason}</span>
              </div>
            )}
          </div>
        </div>
      )}

      {/* Legend */}
      <div className="flex items-center justify-center gap-6 mt-3 text-[11px] text-slate-400">
        <span className="flex items-center gap-1.5">
          <span className="w-3 h-3 rounded-full bg-blue-500 inline-block" /> Coordinator
        </span>
        <span className="flex items-center gap-1.5">
          <span className="w-3 h-3 rounded-full bg-green-500 inline-block" /> Contributor
        </span>
        <span className="flex items-center gap-1.5">
          <span className="w-3 h-3 rounded-full bg-amber-500 inline-block" /> Excluded/Unused
        </span>
      </div>
    </div>
  );
};

export default TopologyMap;
