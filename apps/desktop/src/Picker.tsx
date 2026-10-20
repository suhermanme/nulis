import { useState, type ReactNode } from "react";

export interface PickerItem {
  key: string;
  title: string;
  detail?: string;
  tooltip?: string;
}

interface PickerProps<T extends PickerItem> {
  label: string;
  searchLabel: string;
  placeholder: string;
  query: string;
  onQuery(query: string): void;
  items: readonly T[];
  emptyText: string;
  onChoose(item: T): void;
  onClose(): void;
  footer?: ReactNode;
}

/** A searchable list in the style of the command palette. */
export function Picker<T extends PickerItem>({
  label, searchLabel, placeholder, query, onQuery, items, emptyText, onChoose, onClose, footer,
}: PickerProps<T>) {
  const [index, setIndex] = useState(0);
  const selected = Math.min(index, Math.max(0, items.length - 1));
  return <div className="palette-backdrop" role="presentation"
    onMouseDown={(event) => { if (event.target === event.currentTarget) onClose(); }}>
    <section className="command-palette" role="dialog" aria-modal="true" aria-label={label}>
      <input autoFocus aria-label={searchLabel} placeholder={placeholder} value={query}
        onChange={(event) => { onQuery(event.currentTarget.value); setIndex(0); }}
        onKeyDown={(event) => {
          if (event.key === "ArrowDown") { event.preventDefault(); setIndex(Math.min(selected + 1, items.length - 1)); }
          if (event.key === "ArrowUp") { event.preventDefault(); setIndex(Math.max(selected - 1, 0)); }
          if (event.key === "Enter" && items[selected]) { event.preventDefault(); onChoose(items[selected]); }
          if (event.key === "Escape") onClose();
        }} />
      <div className="palette-results" role="listbox" aria-label={label}>
        {items.map((item, position) => <button type="button" role="option" title={item.tooltip}
          aria-selected={position === selected} className={position === selected ? "selected" : ""}
          key={item.key} onMouseEnter={() => setIndex(position)} onClick={() => onChoose(item)}>
          <span>{item.title}</span>{item.detail ? <small>{item.detail}</small> : null}
        </button>)}
        {items.length === 0 ? <p>{emptyText}</p> : null}
      </div>
      {footer ? <div className="palette-footer">{footer}</div> : null}
    </section>
  </div>;
}
