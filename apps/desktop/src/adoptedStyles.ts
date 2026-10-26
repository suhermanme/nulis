/**
 * Re-applies the inline styles a library writes under `root` through the CSSOM.
 *
 * The app's CSP is `style-src 'self'`, which blocks `<style>` elements created at
 * runtime and `style="…"` attributes set as markup. xterm.js uses both: its font, cell
 * size and cursor rules live in `<style>` elements, and 24-bit colours and contrast
 * lifts are `style` attributes on the row spans. In a packaged build the grid therefore
 * fell back to the page's proportional font and lost its cursor. The dev server serves
 * without that CSP, which is why the problem only showed in the built app.
 *
 * CSP does not gate the CSSOM, so this mirrors each `<style>` element into a
 * constructable stylesheet adopted by the document, and copies a blocked `style`
 * attribute into `element.style.cssText`. The CSP stays as strict as it is.
 *
 * Returns a function that stops mirroring and removes the mirrored sheets.
 */
export function adoptStyleElements(root: HTMLElement): () => void {
  const doc = root.ownerDocument;
  const Sheet = doc.defaultView?.CSSStyleSheet;
  const canAdopt = !!Sheet && "replaceSync" in Sheet.prototype && "adoptedStyleSheets" in doc;
  const mirrored = new Map<HTMLStyleElement, { sheet: CSSStyleSheet; text: string }>();
  // Every sheet this call ever made, so a removed element's sheet leaves the document too.
  const created = new Set<CSSStyleSheet>();

  const publish = () => {
    if (!canAdopt) return;
    const others = doc.adoptedStyleSheets.filter((sheet) => !created.has(sheet));
    doc.adoptedStyleSheets = [...others, ...Array.from(mirrored.values(), (entry) => entry.sheet)];
  };

  const syncSheets = () => {
    if (!canAdopt || !Sheet) return;
    const present = new Set(root.querySelectorAll("style"));
    let changed = false;
    for (const element of mirrored.keys()) {
      if (!present.has(element)) {
        mirrored.delete(element);
        changed = true;
      }
    }
    for (const element of present) {
      const text = element.textContent ?? "";
      let entry = mirrored.get(element);
      if (!entry) {
        entry = { sheet: new Sheet(), text: "" };
        created.add(entry.sheet);
        mirrored.set(element, entry);
        changed = true;
      }
      if (entry.text !== text) {
        try {
          entry.sheet.replaceSync(text);
        } catch {
          // The text was unusable as a whole; the sheet keeps its previous rules.
        }
        entry.text = text;
      }
    }
    if (changed) publish();
  };

  // A blocked attribute stays in the markup but leaves the declaration empty, so an
  // element whose style is already applied (the dev server, or no CSP) is left alone.
  const restoreAttribute = (element: Element) => {
    if (!(element instanceof HTMLElement)) return;
    const declared = element.getAttribute("style");
    if (declared && element.style.length === 0) element.style.cssText = declared;
  };
  const restoreTree = (node: Node) => {
    if (!(node instanceof Element)) return;
    restoreAttribute(node);
    for (const child of node.querySelectorAll("[style]")) restoreAttribute(child);
  };

  const observer = new MutationObserver((records) => {
    let sheets = false;
    for (const record of records) {
      if (record.target instanceof HTMLStyleElement) sheets = true;
      for (const node of [...record.addedNodes, ...record.removedNodes]) {
        if (node instanceof HTMLStyleElement || (node instanceof Element && node.querySelector("style"))) sheets = true;
      }
      for (const node of record.addedNodes) restoreTree(node);
    }
    if (sheets) syncSheets();
  });
  observer.observe(root, { childList: true, subtree: true });
  syncSheets();
  restoreTree(root);

  return () => {
    observer.disconnect();
    mirrored.clear();
    publish();
  };
}
