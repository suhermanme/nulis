import { useEffect, useRef } from "react";
import type { CompletionItem } from "./contracts";

const KIND_MARKS: Record<CompletionItem["kind"], string> = {
  snippet: "S", keyword: "K", type: "T", word: "W",
};
const KIND_NAMES: Record<CompletionItem["kind"], string> = {
  snippet: "snippet", keyword: "keyword", type: "type", word: "word",
};

/** The id of the option at `index`, for `aria-activedescendant`. */
export function completionOptionId(listId: string, index: number): string {
  return `${listId}-option-${index}`;
}

/** Suggestions under the caret. The editor keeps focus; this list only shows and takes clicks. */
export function CompletionPopup({ id, items, index, left, top, onPick, onHover }: {
  id: string;
  items: readonly CompletionItem[];
  index: number;
  left: number;
  top: number;
  onPick(index: number): void;
  onHover(index: number): void;
}) {
  const listRef = useRef<HTMLUListElement>(null);
  useEffect(() => {
    const option = listRef.current?.querySelector<HTMLElement>(`#${CSS.escape(completionOptionId(id, index))}`);
    option?.scrollIntoView?.({ block: "nearest" });
  }, [id, index]);
  return <ul ref={listRef} id={id} className="completion-popup" role="listbox" aria-label="Suggestions"
    style={{ left, top }}>
    {items.map((item, position) => <li key={`${item.kind}-${item.label}-${position}`} id={completionOptionId(id, position)}
      role="option" aria-selected={position === index}
      aria-label={`${item.label}, ${KIND_NAMES[item.kind]}${item.detail ? `, ${item.detail}` : ""}`}
      // Picking must not take focus away from the editor.
      onMouseDown={(event) => { event.preventDefault(); onPick(position); }}
      onMouseMove={() => { if (position !== index) onHover(position); }}>
      <span className={`completion-kind completion-${item.kind}`} aria-hidden="true">{KIND_MARKS[item.kind]}</span>
      <span className="completion-label">{item.label}</span>
      {item.detail ? <span className="completion-detail">{item.detail}</span> : null}
    </li>)}
  </ul>;
}
