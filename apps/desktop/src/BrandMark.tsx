/**
 * The Nulis mark, an N whose last stroke is a text caret, drawn in the
 * current theme's colors: the tile is the raised surface, the N the text
 * color, and the caret the accent. In a dark theme it matches the
 * application icon (`src-tauri/icons/nulis-source.svg`), which keeps its own
 * fixed colors; in a light theme it becomes a light tile with a dark N.
 */
export function BrandMark({ className }: { className?: string }) {
  return <svg className={className} viewBox="100 100 824 824" aria-hidden="true" focusable="false">
    <rect className="mark-tile" x="100" y="100" width="824" height="824" rx="186" />
    <rect className="mark-tile-edge" x="103" y="103" width="818" height="818" rx="183" />
    <path className="mark-letter" d="M296 292 H392 L614 732 H518 L392 482 V732 H296 Z" />
    <rect className="mark-caret" x="636" y="282" width="92" height="460" rx="22" />
  </svg>;
}
