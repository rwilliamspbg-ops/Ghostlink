import { describe, it, expect } from "vitest";
import { computeFitBadge } from "./fitBadge";

describe("computeFitBadge", () => {
  it("classifies Fits this machine when size <= 95% VRAM", () => {
    const res = computeFitBadge(4.0, { vramGb: 8.0, ramGb: 16.0 });
    expect(res.label).toBe("Fits this machine");
    expect(res.tooltipMath).toContain("Model size: 4.0 GB");
  });

  it("classifies Tight on this machine when size spills into RAM", () => {
    const res = computeFitBadge(10.0, { vramGb: 8.0, ramGb: 16.0 });
    expect(res.label).toBe("Tight on this machine");
    expect(res.tooltipMath).toContain("will spill over to CPU RAM");
  });

  it("classifies Needs cluster when local memory is short but cluster aggregate is enough", () => {
    const res = computeFitBadge(30.0, { vramGb: 8.0, ramGb: 16.0, clusterAggregateVramGb: 64.0 });
    expect(res.label).toBe("Needs cluster");
    expect(res.tooltipMath).toContain("requires ggml-rpc cluster sharding");
  });

  it("classifies Likely too big for this machine when local memory is short and no cluster VRAM is probed", () => {
    const res = computeFitBadge(30.0, { vramGb: 8.0, ramGb: 16.0, clusterAggregateVramGb: 0 });
    expect(res.label).toBe("Likely too big for this machine");
    expect(res.tooltipMath).toContain("connect workers in Workers tab");
  });

  it("classifies Will not load when model exceeds cluster aggregate memory", () => {
    const res = computeFitBadge(100.0, { vramGb: 8.0, ramGb: 16.0, clusterAggregateVramGb: 64.0 });
    expect(res.label).toBe("Will not load");
    expect(res.tooltipMath).toContain("exceeds total cluster memory");
  });

  it("classifies Unknown when size is missing or invalid", () => {
    const res = computeFitBadge(null, { vramGb: 8.0 });
    expect(res.label).toBe("Unknown");
  });
});
