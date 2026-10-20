import { useEffect, type RefObject } from "react";

/** Height the scrolling `container` needs to show all of its children without scrolling. */
export function contentHeight(container: HTMLElement): number {
  const last = container.lastElementChild;
  if (!last) return 0;
  const top = container.getBoundingClientRect().top;
  const bottom = last.getBoundingClientRect().bottom;
  const padding = Number.parseFloat(getComputedStyle(container).paddingBottom) || 0;
  return Math.ceil(bottom - top + container.scrollTop + padding);
}

/**
 * Keeps a separate window as tall as its content. Whenever the content of
 * `selector` inside `rootRef` changes height (another tab, options shown or
 * hidden, a list appears), `fit` is asked for that height. Between such
 * changes the window keeps whatever size the user gives it.
 */
export function useFitToContent(rootRef: RefObject<HTMLElement | null>, selector: string, fit: (height: number) => void) {
  useEffect(() => {
    const container = rootRef.current?.querySelector<HTMLElement>(selector);
    if (!container || typeof ResizeObserver === "undefined") return;
    let frame = 0;
    let pending = false;
    let last = 0;
    const measure = () => {
      pending = false;
      const height = contentHeight(container);
      if (height > 0 && Math.abs(height - last) >= 1) {
        last = height;
        fit(height);
      }
    };
    const schedule = () => {
      if (pending) return;
      pending = true;
      frame = requestAnimationFrame(measure);
    };
    const observer = new ResizeObserver(schedule);
    for (const child of Array.from(container.children)) observer.observe(child);
    // Children come and go (each tab has its own); watch for that too.
    const mutations = new MutationObserver(() => {
      observer.disconnect();
      for (const child of Array.from(container.children)) observer.observe(child);
      schedule();
    });
    mutations.observe(container, { childList: true });
    schedule();
    return () => {
      observer.disconnect();
      mutations.disconnect();
      if (pending) cancelAnimationFrame(frame);
    };
  }, [rootRef, selector, fit]);
}
