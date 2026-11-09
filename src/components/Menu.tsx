import { useEffect, useLayoutEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { TreeNode } from "../lib/api";
import { FolderIcon, SearchIcon } from "./Icons";

export type MenuItem =
  | { separator: true }
  | {
      separator?: false;
      label: string;
      icon?: ReactNode;
      hint?: string;
      danger?: boolean;
      disabled?: boolean;
      title?: string;
      onClick: () => void;
    };

/** Menu kontekstowe w miejscu kliknięcia; zamyka się kliknięciem obok, Esc albo przewinięciem. */
export function ContextMenu({ x, y, items, onClose }: { x: number; y: number; items: MenuItem[]; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState({ x, y });
  const [active, setActive] = useState(-1);
  const actionable = items.map((it, i) => (!it.separator && !it.disabled ? i : -1)).filter((i) => i >= 0);

  // Menu nie może wyjść poza okno.
  useLayoutEffect(() => {
    const r = ref.current?.getBoundingClientRect();
    if (!r) return;
    setPos({
      x: Math.max(4, Math.min(x, window.innerWidth - r.width - 4)),
      y: Math.max(4, Math.min(y, window.innerHeight - r.height - 4)),
    });
  }, [x, y]);

  useEffect(() => {
    ref.current?.focus();
    const close = (e: Event) => {
      if (e.type === "mousedown" && ref.current?.contains(e.target as Node)) return;
      onClose();
    };
    window.addEventListener("mousedown", close);
    window.addEventListener("blur", close);
    window.addEventListener("resize", close);
    document.addEventListener("scroll", close, true);
    return () => {
      window.removeEventListener("mousedown", close);
      window.removeEventListener("blur", close);
      window.removeEventListener("resize", close);
      document.removeEventListener("scroll", close, true);
    };
  }, [onClose]);

  const run = (i: number) => {
    const it = items[i];
    if (!it || it.separator || it.disabled) return;
    onClose();
    it.onClick();
  };

  return (
    <div
      ref={ref}
      className="ctx-menu"
      role="menu"
      tabIndex={-1}
      style={{ left: pos.x, top: pos.y }}
      onContextMenu={(e) => e.preventDefault()}
      onKeyDown={(e) => {
        const k = actionable.indexOf(active);
        if (e.key === "Escape") onClose();
        else if (e.key === "ArrowDown") setActive(actionable[(k + 1) % actionable.length]);
        else if (e.key === "ArrowUp") setActive(actionable[(k - 1 + actionable.length) % actionable.length]);
        else if (e.key === "Enter") run(active);
        else return;
        e.preventDefault();
      }}
    >
      {items.map((it, i) =>
        it.separator ? (
          <div key={i} className="ctx-sep" role="separator" />
        ) : (
          <button
            key={i}
            role="menuitem"
            className={`ctx-item${it.danger ? " danger" : ""}${active === i ? " active" : ""}`}
            disabled={it.disabled}
            title={it.title}
            onMouseEnter={() => setActive(i)}
            onClick={() => run(i)}
          >
            <span className="ctx-icon">{it.icon}</span>
            <span className="ctx-label">{it.label}</span>
            {it.hint && <span className="ctx-hint">{it.hint}</span>}
          </button>
        ),
      )}
    </div>
  );
}

function Modal({ title, children, onClose }: { title: string; children: ReactNode; onClose: () => void }) {
  return (
    <div className="modal-back" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className="modal" role="dialog" aria-label={title} onKeyDown={(e) => e.key === "Escape" && onClose()}>
        <h2>{title}</h2>
        {children}
      </div>
    </div>
  );
}

/** Okno z polem tekstowym (nazwa, tytuł). */
export function PromptDialog(props: {
  title: string;
  label: string;
  initial: string;
  okLabel: string;
  hint?: string;
  onOk: (value: string) => void;
  onClose: () => void;
}) {
  const [v, setV] = useState(props.initial);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);
  const ok = () => {
    if (!v.trim()) return;
    props.onClose();
    props.onOk(v.trim());
  };
  return (
    <Modal title={props.title} onClose={props.onClose}>
      <label className="modal-label">{props.label}</label>
      <input ref={input} value={v} onChange={(e) => setV(e.target.value)} onKeyDown={(e) => e.key === "Enter" && ok()} />
      {props.hint && <div className="muted small modal-hint">{props.hint}</div>}
      <div className="actions">
        <button onClick={props.onClose}>Anuluj</button>
        <button className="primary" onClick={ok} disabled={!v.trim()}>
          {props.okLabel}
        </button>
      </div>
    </Modal>
  );
}

function norm(s: string): string {
  return s.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/** Wybór Folderu docelowego (z filtrem). */
export function FolderPicker(props: {
  title: string;
  tree: TreeNode[];
  current: string;
  onPick: (folder: string) => void;
  onClose: () => void;
}) {
  const [q, setQ] = useState("");
  const [sel, setSel] = useState<string | null>(null);
  const folders = useMemo(() => {
    const out: { path: string; depth: number; title: string }[] = [{ path: "", depth: 0, title: "Skarbiec (główny katalog)" }];
    const walk = (list: TreeNode[], d: number) =>
      list.forEach((n) => {
        if (n.kind === "folder") {
          out.push({ path: n.path, depth: d, title: n.title });
          walk(n.children, d + 1);
        }
      });
    walk(props.tree, 1);
    return out;
  }, [props.tree]);
  const nq = norm(q.trim());
  const shown = nq ? folders.filter((f) => norm(f.path).includes(nq)) : folders;
  const ok = (f: string | null) => {
    if (f === null || f === props.current) return;
    props.onClose();
    props.onPick(f);
  };
  return (
    <Modal title={props.title} onClose={props.onClose}>
      <label className="search-field">
        <SearchIcon size={14} />
        <input
          autoFocus
          placeholder="Szukaj Folderu…"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && ok(sel ?? shown.find((f) => f.path !== props.current)?.path ?? null)}
        />
      </label>
      <div className="picker-list">
        {shown.map((f) => (
          <div
            key={f.path || "/"}
            className={`tree-row folder${sel === f.path ? " active" : ""}${f.path === props.current ? " disabled" : ""}`}
            style={{ paddingLeft: nq ? 8 : 8 + f.depth * 14 }}
            title={f.path === props.current ? "Notatka już jest w tym Folderze" : f.path || "Skarbiec"}
            onClick={() => setSel(f.path)}
            onDoubleClick={() => ok(f.path)}
          >
            <span className="tree-icon">
              <FolderIcon size={15} />
            </span>
            <span className="tree-label">{nq && f.path ? f.path.replace(/\//g, " › ") : f.title}</span>
          </div>
        ))}
      </div>
      <div className="actions">
        <button onClick={props.onClose}>Anuluj</button>
        <button className="primary" disabled={sel === null || sel === props.current} onClick={() => ok(sel)}>
          Przenieś tutaj
        </button>
      </div>
    </Modal>
  );
}
