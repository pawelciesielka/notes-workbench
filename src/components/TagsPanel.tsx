import { useEffect, useState } from "react";
import { api, type NoteRef, type Summary } from "../lib/api";

type Props = {
  summary: Summary | null;
  selected: string | null;
  activeNote: string | null;
  onSelect: (tag: string | null) => void;
  onOpen: (path: string) => void;
};

export function TagsPanel({ summary, selected, activeNote, onSelect, onOpen }: Props) {
  const [notes, setNotes] = useState<NoteRef[]>([]);

  useEffect(() => {
    if (!selected) return setNotes([]);
    api.tagNotes(selected).then(setNotes).catch(() => setNotes([]));
  }, [selected, summary]);

  const tags = summary?.tags ?? [];
  if (!tags.length)
    return (
      <div className="muted pad">
        Brak Tagów. Dodaj Tag, pisząc w treści Notatki <code>#nazwa</code> albo we frontmatterze <code>tags: [nazwa]</code>.
      </div>
    );

  return (
    <div className="tree-list">
      {tags.map((t) => {
        const on = selected?.toLowerCase() === t.tag.toLowerCase();
        return (
          <div key={t.tag}>
            <div className={`tree-row${on ? " active" : ""}`} onClick={() => onSelect(on ? null : t.tag)}>
              <span className="tree-icon">{on ? "▾" : "▸"}</span>
              <span className="tree-label">#{t.tag}</span>
              <span className="count">{t.count}</span>
            </div>
            {on &&
              notes.map((n) => (
                <div
                  key={n.path}
                  className={`tree-row${activeNote === n.path ? " active" : ""}`}
                  style={{ paddingLeft: 30 }}
                  title={n.path}
                  onClick={() => onOpen(n.path)}
                >
                  <span className="tree-label">{n.title}</span>
                </div>
              ))}
          </div>
        );
      })}
    </div>
  );
}
