import { useEffect, useMemo, useRef, useState, type CSSProperties, type KeyboardEvent } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import userGuide from "../../../docs/USER_GUIDE.md?raw";
import pluginGuide from "../../../docs/PLUGIN_GUIDE.md?raw";
import { Icon, type IconName } from "./icons";

export interface HelpSection {
  id: string;
  title: string;
  icon: IconName;
  /** Markdown body, without the section's own heading. */
  body: string;
}

const SECTION_ICONS: Record<string, IconName> = {
  "Getting Started": "general", "Files": "document", "Tabs and Workspaces": "workspace", "Editing": "edit",
  "Search": "find", "Languages and Encodings": "language", "Tools": "validate", "File View and Remote": "folders",
  "Automation": "automation", "Plugins": "plugins", "Building Plugins": "symbols", "Settings and Themes": "settings",
  "Keyboard Shortcuts": "keyboard", "Limits": "column", "About and License": "security",
};
const SECTION_KEY = "nulis.help.section.v1";

function slug(title: string): string {
  return title.toLocaleLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
}

function section(title: string, body: string): HelpSection {
  return { id: slug(title), title, icon: SECTION_ICONS[title] ?? "document", body: body.trim() };
}

/** Each second-level heading of the user guide is one section. */
function chapters(markdown: string): HelpSection[] {
  return markdown.split(/^## /m).slice(1).map((part) => {
    const end = part.indexOf("\n");
    return section(part.slice(0, end).trim(), part.slice(end + 1));
  });
}

/** A whole document as one section, titled by its first-level heading. */
function wholeDocument(markdown: string): HelpSection {
  const match = /^# (.+)\n/.exec(markdown);
  return section(match?.[1].trim() ?? "Guide", match ? markdown.slice(match[0].length) : markdown);
}

/** Source files of the help, by the name other guides link to them with. */
const DOCUMENT_SECTIONS: Record<string, string> = { "PLUGIN_GUIDE.md": "Building Plugins" };

/**
 * Where a link inside the help leads: another section, a heading of the
 * current section, or nowhere (an outside address, shown as plain text).
 */
function linkTarget(href: string | undefined): { section?: string; heading?: string } | null {
  if (!href || /^[a-z][a-z0-9+.-]*:/i.test(href)) return null;
  const [path, heading] = href.split("#", 2);
  if (!path) return heading ? { heading } : null;
  const title = DOCUMENT_SECTIONS[path.split("/").pop() ?? ""];
  const target = HELP_SECTIONS.find((item) => item.title === title);
  return target ? { section: target.id, heading } : null;
}

/** The bundled help: the user guide's chapters, with the plugin developer guide after "Plugins". */
export const HELP_SECTIONS: readonly HelpSection[] = (() => {
  const sections = chapters(userGuide);
  const after = sections.findIndex((item) => item.title === "Plugins");
  sections.splice(after < 0 ? sections.length : after + 1, 0, wholeDocument(pluginGuide));
  return sections;
})();

function loadSection(): string {
  try {
    const stored = window.localStorage.getItem(SECTION_KEY);
    return HELP_SECTIONS.find((item) => item.id === stored)?.id ?? HELP_SECTIONS[0].id;
  } catch {
    return HELP_SECTIONS[0].id;
  }
}

interface HelpWindowProps {
  /** `window` fills its own desktop window; `inline` is a dialog over the workbench. */
  mode: "window" | "inline";
  /** Theme variables, for the separate window; the inline dialog inherits the workbench's. */
  style?: CSSProperties;
  onClose(): void;
  /** Shows this section and heading; a new `id` shows it again. */
  topic?: { id: number; section: string; heading?: string };
}

/** Sectioned help, laid out like the Settings window. */
export function HelpWindow({ mode, style, onClose, topic }: HelpWindowProps) {
  const [current, setCurrent] = useState(loadSection);
  const [query, setQuery] = useState("");
  const scrollRef = useRef<HTMLDivElement>(null);
  const needle = query.trim().toLocaleLowerCase();
  const visible = useMemo(() => HELP_SECTIONS.filter((item) => !needle
    || item.title.toLocaleLowerCase().includes(needle) || item.body.toLocaleLowerCase().includes(needle)), [needle]);
  const shown = HELP_SECTIONS.find((item) => item.id === current) ?? HELP_SECTIONS[0];

  useEffect(() => {
    try { window.localStorage.setItem(SECTION_KEY, current); } catch { /* Navigation still works without storage. */ }
    if (scrollRef.current) scrollRef.current.scrollTop = 0;
  }, [current]);

  const [pendingHeading, setPendingHeading] = useState<string | null>(null);
  // A link names a heading by its slug, the way a Markdown page on the web does.
  useEffect(() => {
    if (!pendingHeading) return;
    const heading = [...scrollRef.current?.querySelectorAll<HTMLElement>("h2, h3, h4") ?? []]
      .find((item) => slug(item.textContent ?? "") === pendingHeading);
    if (heading && typeof heading.scrollIntoView === "function") heading.scrollIntoView({ block: "start" });
    setPendingHeading(null);
  }, [pendingHeading, current]);
  const follow = (target: { section?: string; heading?: string }) => {
    if (target.section) { setQuery(""); setCurrent(target.section); }
    if (target.heading) setPendingHeading(target.heading);
  };
  useEffect(() => {
    if (topic && HELP_SECTIONS.some((item) => item.id === topic.section)) follow(topic);
  }, [topic?.id]);

  const onNavigationKey = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = visible.findIndex((item) => item.id === current);
    const target = event.key === "ArrowDown" ? index + 1 : event.key === "ArrowUp" ? index - 1
      : event.key === "Home" ? 0 : event.key === "End" ? visible.length - 1 : -1;
    const next = visible[target];
    if (!next) return;
    event.preventDefault();
    setCurrent(next.id);
    event.currentTarget.querySelector<HTMLElement>(`[data-section="${next.id}"]`)?.focus();
  };

  return <div className={`settings-window help-window ${mode}`} style={style} role={mode === "inline" ? "dialog" : undefined}
    aria-modal={mode === "inline" ? true : undefined} aria-label="Nulis Help"
    onKeyDown={(event) => { if (event.key === "Escape") { event.stopPropagation(); onClose(); } }}>
    <nav className="settings-sidebar" aria-label="Help sections">
      <div className="settings-sidebar-title">Nulis Help</div>
      <input className="settings-filter help-search" type="search" aria-label="Search help" placeholder="Search help"
        value={query} onChange={(event) => setQuery(event.currentTarget.value)} />
      <div role="tablist" aria-orientation="vertical" aria-label="Help sections" onKeyDown={onNavigationKey}>
        {visible.map((item) => <button type="button" role="tab" key={item.id} data-section={item.id}
          id={`help-tab-${item.id}`} aria-controls="help-detail" aria-selected={item.id === shown.id}
          tabIndex={item.id === shown.id ? 0 : -1} className={item.id === shown.id ? "active" : ""}
          onClick={() => setCurrent(item.id)}>
          <Icon name={item.icon} size={18} /><span>{item.title}</span>
        </button>)}
      </div>
      {visible.length === 0 ? <p className="help-empty" role="status">No section mentions “{query.trim()}”.</p> : null}
    </nav>
    <section className="settings-detail" id="help-detail" role="tabpanel" aria-labelledby={`help-tab-${shown.id}`}>
      <header>
        <h2>{shown.title}</h2>
        {mode === "inline" ? <button autoFocus type="button" onClick={onClose}>Close</button> : null}
      </header>
      <div className="settings-scroll" ref={scrollRef} tabIndex={0}>
        <article className="markdown-preview help-guide" aria-label={shown.title}>
          <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml
            components={{
              // Help is self-contained: a link opens another section or a heading here, and an
              // outside address is shown as text so the window never navigates away.
              a: ({ href, children }) => {
                const target = linkTarget(href);
                return target
                  ? <button type="button" className="help-link" onClick={() => follow(target)}>{children}</button>
                  : <span className="help-external">{children}</span>;
              },
            }}>
            {shown.body}
          </ReactMarkdown>
        </article>
      </div>
    </section>
  </div>;
}
