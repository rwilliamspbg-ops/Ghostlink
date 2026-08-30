export type FitBadgeLabel =
  | "Fits this machine"
  | "Tight on this machine"
  | "Needs cluster"
  | "Likely too big for this machine"
  | "Will not load"
  | "Unknown";

export interface FitBadgeResult {
  label: FitBadgeLabel;
  badgeClass: string;
  tooltipMath: string;
}

export interface HardwareProbeInput {
  vramGb?: number | null;
  ramGb?: number | null;
  clusterAggregateVramGb?: number | null;
}

export function computeFitBadge(
  sizeGb: number | null | undefined,
  probe: HardwareProbeInput
): FitBadgeResult {
  if (sizeGb == null || sizeGb <= 0) {
    return {
      label: "Unknown",
      badgeClass: "bg-slate-800 text-slate-300 border-slate-700",
      tooltipMath: "Model size or hardware memory probe unavailable.",
    };
  }

  const vram = probe.vramGb ?? 0;
  const ram = probe.ramGb ?? 0;
  const clusterVram = probe.clusterAggregateVramGb ?? 0;

  const tooltipParts: string[] = [
    `Model size: ${sizeGb.toFixed(1)} GB`,
    `Local VRAM: ${vram.toFixed(1)} GB`,
    `Local RAM: ${ram.toFixed(1)} GB`,
  ];
  if (clusterVram > 0) {
    tooltipParts.push(`Cluster Aggregate: ${clusterVram.toFixed(1)} GB`);
  }

  const tooltipMath = tooltipParts.join(" · ");

  if (vram > 0 && sizeGb <= vram * 0.95) {
    return {
      label: "Fits this machine",
      badgeClass: "bg-emerald-500/10 text-emerald-400 border-emerald-500/30",
      tooltipMath,
    };
  }

  const localTotal = vram + ram;
  if (localTotal > 0 && sizeGb <= localTotal * 0.9) {
    return {
      label: "Tight on this machine",
      badgeClass: "bg-amber-500/10 text-amber-400 border-amber-500/30",
      tooltipMath: `${tooltipMath} (will spill over to CPU RAM)`,
    };
  }

  if (clusterVram > 0 && sizeGb <= clusterVram * 0.9) {
    return {
      label: "Needs cluster",
      badgeClass: "bg-indigo-500/10 text-indigo-400 border-indigo-500/30",
      tooltipMath: `${tooltipMath} (requires ggml-rpc cluster sharding across ${clusterVram.toFixed(1)} GB total memory)`,
    };
  }

  if (clusterVram > 0) {
    return {
      label: "Will not load",
      badgeClass: "bg-rose-500/10 text-rose-400 border-rose-500/30",
      tooltipMath: `${tooltipMath} (exceeds total cluster memory)`,
    };
  }

  return {
    label: "Likely too big for this machine",
    badgeClass: "bg-orange-500/10 text-orange-400 border-orange-500/30",
    tooltipMath: `${tooltipMath} (exceeds local memory; connect workers in Workers tab)`,
  };
}
