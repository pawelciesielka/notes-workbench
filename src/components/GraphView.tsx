import { useEffect, useMemo, useRef, useState } from "react";
import ForceGraph from "force-graph";
import { api, type Graph } from "../lib/api";

type Props = {
  /** Notatka, wokół której pokazujemy graf lokalny. */
  center: string | null;
  onOpen: (path: string) => void;
  onTag: (tag: string) => void;
  onError: (msg: string) => void;
};

type N = { id: string; label: string; kind: "note" | "tag"; deg: number; x?: number; y?: number };
type L = { source: string | N; target: string | N };

function css(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || "#888";
}

/** Węzły w odległości co najwyżej `depth` od środka (Tagi liczą się jako zwykłe węzły). */
function neighbourhood(g: Graph, center: string, depth: number): Set<string> {
  const adj = new Map<string, string[]>();
  for (const [a, b] of g.edges) {
    (adj.get(a) ?? adj.set(a, []).get(a)!).push(b);
    (adj.get(b) ?? adj.set(b, []).get(b)!).push(a);
  }
  const seen = new Set([center]);
  let frontier = [center];
  for (let d = 0; d < depth; d++) {
    const next: string[] = [];
    for (const n of frontier) for (const m of adj.get(n) ?? []) if (!seen.has(m)) (seen.add(m), next.push(m));
    frontier = next;
  }
  return seen;
}

export function GraphView({ center, onOpen, onTag, onError }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const [graph, setGraph] = useState<Graph | null>(null);
  const [local, setLocal] = useState(!!center);
  const [depth, setDepth] = useState(2);
  const [showTags, setShowTags] = useState(true);
  const [orphans, setOrphans] = useState(false);

  useEffect(() => {
    api.graph().then(setGraph).catch((e) => onError(String(e)));
  }, [onError]);

  const data = useMemo(() => {
    if (!graph) return null;
    let edges = graph.edges.filter(([a, b]) => showTags || (!a.startsWith("#") && !b.startsWith("#")));
    let nodes = graph.nodes.filter((n) => showTags || n.kind !== "tag");
    if (local && center) {
      const keep = neighbourhood({ nodes, edges }, center, depth);
      nodes = nodes.filter((n) => keep.has(n.id));
      edges = edges.filter(([a, b]) => keep.has(a) && keep.has(b));
    }
    const deg = new Map<string, number>();
    for (const [a, b] of edges) {
      deg.set(a, (deg.get(a) ?? 0) + 1);
      deg.set(b, (deg.get(b) ?? 0) + 1);
    }
    if (!orphans && !(local && center)) nodes = nodes.filter((n) => deg.has(n.id));
    return {
      nodes: nodes.map((n) => ({ ...n, deg: deg.get(n.id) ?? 0 })) as N[],
      links: edges.map(([source, target]) => ({ source, target })) as L[],
    };
  }, [graph, local, center, depth, showTags, orphans]);

  useEffect(() => {
    if (!host.current || !data) return;
    const el = host.current;
    const colors = { note: css("--accent"), tag: css("--tag"), center: css("--center"), fg: css("--fg"), edge: css("--border") };
    let hover: string | null = null;
    const fg = new ForceGraph<N, L>(el)
      .width(el.clientWidth)
      .height(el.clientHeight)
      .graphData(data)
      .nodeId("id")
      .nodeVal((n) => 1 + Math.sqrt(n.deg))
      .nodeColor((n) => (n.id === center ? colors.center : n.kind === "tag" ? colors.tag : colors.note))
      .nodeLabel((n) => n.label)
      .linkColor(() => colors.edge)
      .onNodeHover((n) => {
        hover = n?.id ?? null;
        el.style.cursor = n ? "pointer" : "default";
      })
      .onNodeClick((n) => (n.kind === "tag" ? onTag(n.label.replace(/^#/, "")) : onOpen(n.id)))
      .nodeCanvasObjectMode(() => "after")
      .nodeCanvasObject((n, ctx, scale) => {
        // Podpisy tylko przy przybliżeniu, dla środka i najechanego węzła; inaczej graf jest nieczytelny.
        if (scale < 1.6 && n.id !== center && n.id !== hover && data.nodes.length > 40) return;
        const size = 11 / scale;
        ctx.font = `${size}px Segoe UI, sans-serif`;
        ctx.fillStyle = colors.fg;
        ctx.textAlign = "center";
        ctx.textBaseline = "top";
        const r = Math.sqrt(1 + Math.sqrt(n.deg)) * 4;
        const label = n.label.length > 40 ? n.label.slice(0, 39) + "…" : n.label;
        ctx.fillText(label, n.x ?? 0, (n.y ?? 0) + r + 1);
      })
      .cooldownTicks(200);
    fg.onEngineStop(() => fg.zoomToFit(400, 40));
    const ro = new ResizeObserver(() => fg.width(el.clientWidth).height(el.clientHeight));
    ro.observe(el);
    return () => {
      ro.disconnect();
      fg._destructor();
      el.innerHTML = "";
    };
  }, [data, center, onOpen, onTag]);

  const count = data ? `${data.nodes.length} węzłów, ${data.links.length} krawędzi` : "";

  return (
    <div className="graph-view">
      <div className="graph-bar">
        <div className="tabs">
          <button className={`tab${local && center ? " on" : ""}`} disabled={!center} onClick={() => setLocal(true)}>
            Wokół Notatki
          </button>
          <button className={`tab${!(local && center) ? " on" : ""}`} onClick={() => setLocal(false)}>
            Cały Skarbiec
          </button>
        </div>
        {local && center && (
          <label>
            Głębokość{" "}
            <select value={depth} onChange={(e) => setDepth(Number(e.target.value))}>
              {[1, 2, 3].map((d) => (
                <option key={d}>{d}</option>
              ))}
            </select>
          </label>
        )}
        <label>
          <input type="checkbox" checked={showTags} onChange={(e) => setShowTags(e.target.checked)} /> Tagi
        </label>
        {!(local && center) && (
          <label>
            <input type="checkbox" checked={orphans} onChange={(e) => setOrphans(e.target.checked)} /> Notatki bez
            połączeń
          </label>
        )}
        <span className="muted">{count}</span>
      </div>
      {data && data.nodes.length === 0 ? (
        <div className="empty muted pad graph-empty">
          Graf jest pusty: żadna Notatka nie ma jeszcze Linków ani Tagów. Wpisz w Notatce <code>[[</code>, aby wstawić Link,
          albo <code>#tag</code>, aby dodać Tag.
        </div>
      ) : (
        <div className="graph-canvas" ref={host} />
      )}
    </div>
  );
}
