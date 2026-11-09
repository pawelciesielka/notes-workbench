import { useEffect, useState } from "react";
import { api, type NoteRef, type Summary } from "../lib/api";
import { ChevronRight, FileIcon, TagIcon } from "./Icons";

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
            <div className="tree-row folder" onClick={() => onSelect(on ? null : t.tag)}>
              <span className={`tree-chevron${on ? " open" : ""}`}>
                <ChevronRight size={14} />
              </span>
              <span className="tree-icon tag-icon">
                <TagIcon size={14} />
              </span>
              <span className="tree-label">{t.tag}</span>
              <span className="tree-count">{t.count}</span>
            </div>
            {on && (
              <div className="tree-children">
                {notes.map((n) => (
                  <div
                    key={n.path}
                    className={`tree-row leaf${activeNote === n.path ? " active" : ""}`}
                    title={n.path}
                    onClick={() => onOpen(n.path)}
                  >
                    <span className="tree-chevron spacer" />
                    <span className="tree-icon">
                      <FileIcon size={15} />
                    </span>
                    <span className="tree-label">{n.title}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}
