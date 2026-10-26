import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { Icon, IconButton, type IconName } from "./icons";

// 60 px is the budget for one labeled control, used only before the controls are measured:
// the real widths come from the measuring layer below, and the bar's own column gap counts too.
const LABELED_ITEM_WIDTH = 60;
const MORE_LABEL = "More toolbar actions";
const MORE_CAPTION = "More";
const CUSTOMIZE_LABEL = "Customize Toolbar…";

export interface ToolbarItem {
  id: string;
  /** Accessible name; the tooltip adds the shortcut. */
  label: string;
  /** Plain action title for the overflow menu. */
  title: string;
  icon: IconName;
  /** Short text shown under the icon. */
  caption?: string;
  shortcut?: string;
  disabled?: boolean;
  pressed?: boolean;
  /** Essential actions stay visible when low-priority ones move to the overflow menu. */
  essential?: boolean;
  onSelect(): void;
}

interface Measured {
  /** The bar's own column gap, in px. 0 when the browser reports none (jsdom, first paint). */
  gap: number;
  /** Each control's natural width, keyed by id, gap excluded. */
  item: Record<string, number>;
  /** The overflow control's natural width, gap excluded. */
  more: number;
}

const UNMEASURED: Measured = { gap: 0, item: {}, more: LABELED_ITEM_WIDTH };

const sameMeasured = (a: Measured, b: Measured) =>
  a.gap === b.gap && a.more === b.more &&
  Object.keys(a.item).length === Object.keys(b.item).length &&
  Object.keys(a.item).every((id) => a.item[id] === b.item[id]);

/**
 * Icon toolbar that never deforms its icons: at narrow widths the
 * lowest-priority controls move into an overflow menu instead.
 */
export function OverflowToolbar({ items, label, className, onCustomize }: {
  items: ToolbarItem[]; label: string; className?: string; onCustomize?(): void;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const measureRef = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  const [measured, setMeasured] = useState<Measured>(UNMEASURED);
  const [open, setOpen] = useState(false);

  useEffect(() => {
    const container = containerRef.current;
    if (!container || typeof ResizeObserver === "undefined") return;
    setWidth(container.clientWidth);
    const observer = new ResizeObserver(([entry]) => { if (entry) setWidth(entry.contentRect.width); });
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  // A hidden twin of the bar's controls, laid out at their natural widths: the fit below
  // spends the room the controls actually take, gaps included, instead of a guessed budget.
  useLayoutEffect(() => {
    const layer = measureRef.current;
    if (!layer) return;
    const gap = Number.parseFloat(getComputedStyle(layer).columnGap) || 0;
    const item: Record<string, number> = {};
    for (const node of layer.querySelectorAll<HTMLElement>("[data-measure-item]")) {
      const id = node.dataset.measureItem;
      if (id) item[id] = node.offsetWidth || LABELED_ITEM_WIDTH;
    }
    const next: Measured = {
      gap,
      item,
      more: layer.querySelector<HTMLElement>("[data-measure-more]")?.offsetWidth || LABELED_ITEM_WIDTH,
    };
    setMeasured((current) => sameMeasured(current, next) ? current : next);
  }, [items]);

  const widthOf = (item: ToolbarItem) => measured.item[item.id] || LABELED_ITEM_WIDTH;
  const gap = measured.gap;
  // The largest group of controls that fits beside the overflow control, essentials first.
  const ranked = [...items.filter((item) => item.essential), ...items.filter((item) => !item.essential)];
  let room = 0;
  if (width > 0) {
    let used = 0;
    for (const item of ranked) {
      const next = used + widthOf(item) + (room > 0 ? gap : 0);
      if (next + gap + measured.more > width) break;
      used = next;
      room += 1;
    }
  } else {
    // An unmeasured toolbar (first paint, tests without a width) shows everything.
    room = items.length;
  }
  const hidden = new Set<string>(room < ranked.length ? ranked.slice(Math.max(1, room)).map((item) => item.id) : []);
  const overflow = items.filter((item) => hidden.has(item.id));

  // A row names itself with its short word, and the tooltip carries the full title with it.
  const rowTooltip = (item: ToolbarItem) => {
    const title = item.caption && item.caption !== item.title ? `${item.title} — ${item.caption}` : item.title;
    return item.shortcut ? `${title} (${item.shortcut})` : title;
  };

  return <>
    <div className={`icon-toolbar${className ? ` ${className}` : ""}`} role="toolbar" aria-label={label} ref={containerRef}>
      {items.filter((item) => !hidden.has(item.id)).map((item) => <IconButton key={item.id} icon={item.icon}
        label={item.label} tooltip={item.title} caption={item.caption ?? item.title} shortcut={item.shortcut}
        pressed={item.pressed} disabled={item.disabled} onClick={item.onSelect} />)}
      {overflow.length > 0 ? <div className="toolbar-overflow" onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
      }}>
        <button type="button" className="icon-button captioned" aria-label={MORE_LABEL}
          title={`${MORE_CAPTION} — Show extra toolbar items`}
          aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen((shown) => !shown)}>
          <Icon name="chevronRight" /><span className="icon-caption" aria-hidden="true">{MORE_CAPTION}</span>
        </button>
        {open ? <div className="file-menu-popover overflow-menu" role="menu" aria-label={MORE_LABEL}>
          {overflow.map((item) => <button type="button" key={item.id} disabled={item.disabled}
            role={item.pressed === undefined ? "menuitem" : "menuitemcheckbox"} aria-label={item.label} aria-checked={item.pressed} title={rowTooltip(item)}
            onClick={() => { setOpen(false); item.onSelect(); }}>
            <Icon name={item.icon} /><span className="menu-caption">{item.caption ?? item.title}</span>
            {item.shortcut ? <kbd>{item.shortcut}</kbd> : null}
          </button>)}
          {onCustomize ? <>
            <div className="menu-separator" role="separator" />
            <button type="button" role="menuitem" aria-label={CUSTOMIZE_LABEL} title={CUSTOMIZE_LABEL}
              onClick={() => { setOpen(false); onCustomize(); }}>
              <Icon name="settings" /><span className="menu-caption">{CUSTOMIZE_LABEL}</span>
            </button>
          </> : null}
        </div> : null}
      </div> : null}
    </div>
    {/* Off-screen twin of every control, in the bar's own flex row: hidden from sight,
        from assistive technology (aria-hidden), and from layout (absolutely positioned). */}
    <div className="icon-toolbar toolbar-measure" aria-hidden="true" ref={measureRef}>
      {items.map((item) => <IconButton key={item.id} data-measure-item={item.id} icon={item.icon}
        label={item.label} caption={item.caption ?? item.title} pressed={item.pressed} disabled={item.disabled} />)}
      <IconButton data-measure-more icon="chevronRight" label={MORE_LABEL} caption={MORE_CAPTION} />
    </div>
  </>;
}

export type MenuItem = { kind: "separator" } | {
  kind?: "command";
  label: string;
  shortcut?: string;
  checked?: boolean;
  disabled?: boolean;
  onSelect(): void;
};

/** Browser-fallback menu; the desktop shell uses the native menu bar instead. */
export function MenuButton({ name, items, children }: { name: string; items: MenuItem[]; children?: ReactNode }) {
  const [open, setOpen] = useState(false);
  return <div className="file-menu" onBlur={(event) => {
    if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
  }} onKeyDown={(event) => { if (event.key === "Escape") setOpen(false); }}>
    <button className="menu-trigger" type="button" aria-label={`Open ${name} menu`}
      aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen((shown) => !shown)}>{children ?? name}</button>
    {open ? <div className="file-menu-popover" role="menu" aria-label={name}>
      {items.map((item, index) => item.kind === "separator"
        ? <div className="menu-separator" role="separator" key={`separator-${index}`} />
        : <button type="button" key={`${item.label}-${index}`} disabled={item.disabled}
          role={item.checked === undefined ? "menuitem" : "menuitemcheckbox"} aria-checked={item.checked}
          onClick={() => { setOpen(false); item.onSelect(); }}>
          <span>{item.checked ? "✓ " : ""}{item.label}</span><kbd>{item.shortcut ?? ""}</kbd>
        </button>)}
    </div> : null}
  </div>;
}

/** A menu opened at the pointer; it closes on a choice, Escape, or a click elsewhere. */
export function ContextMenu({ x, y, label, items, onClose }: {
  x: number; y: number; label: string; items: MenuItem[]; onClose(): void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ left: x, top: y });
  // Keep the whole menu inside the window.
  useLayoutEffect(() => {
    const menu = ref.current;
    if (!menu) return;
    setPosition({
      left: Math.max(4, Math.min(x, window.innerWidth - menu.offsetWidth - 4)),
      top: Math.max(4, Math.min(y, window.innerHeight - menu.offsetHeight - 4)),
    });
    menu.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
  }, [x, y]);
  useEffect(() => {
    const dismiss = (event: Event) => {
      if (!(event.target instanceof Node) || !ref.current?.contains(event.target)) onClose();
    };
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") onClose(); };
    window.addEventListener("pointerdown", dismiss, true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("blur", onClose);
    window.addEventListener("resize", onClose);
    return () => {
      window.removeEventListener("pointerdown", dismiss, true);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("blur", onClose);
      window.removeEventListener("resize", onClose);
    };
  }, [onClose]);
  return <div className="file-menu-popover context-menu" role="menu" aria-label={label} ref={ref} style={position}
    onKeyDown={(event) => {
      const offset = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
      if (!offset) return;
      event.preventDefault();
      const buttons = [...event.currentTarget.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
      const index = buttons.indexOf(document.activeElement as HTMLButtonElement);
      buttons[(index + offset + buttons.length) % buttons.length]?.focus();
    }}>
    {items.map((item, index) => item.kind === "separator"
      ? <div className="menu-separator" role="separator" key={`separator-${index}`} />
      : <button type="button" role="menuitem" key={`${item.label}-${index}`} disabled={item.disabled}
        onClick={() => { onClose(); item.onSelect(); }}>
        <span>{item.label}</span><kbd>{item.shortcut ?? ""}</kbd>
      </button>)}
  </div>;
}
