import { useEffect, useState, useSyncExternalStore } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { DEFAULT_REMOTE_IMAGE_POLICY, type RemoteImagePolicy } from "./contracts";
import { getMarkdownPreviewImage } from "./ipc";

/** Reads an image relative to the document as a data URL. */
export type ImageLoader = (documentId: string, relativePath: string) => Promise<string>;

// Documents whose remote images the reader chose to load ("ask" mode). Document
// IDs are never reused, so an approval ends with the document it was given for.
const approvedDocuments = new Set<string>();
const listeners = new Set<() => void>();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => { listeners.delete(listener); };
}

/** Loads the remote images of one document in every preview that shows it. */
export function approveRemoteImages(documentId: string): void {
  if (approvedDocuments.has(documentId)) return;
  approvedDocuments.add(documentId);
  for (const listener of listeners) listener();
}

/** Whether a document's secure remote images may load under a policy. */
export function remoteImagesAllowed(policy: RemoteImagePolicy, documentId: string): boolean {
  return policy === "allow" || (policy === "ask" && approvedDocuments.has(documentId));
}

/** Forgets every approval. For tests. */
export function resetRemoteImageApprovals(): void {
  approvedDocuments.clear();
  for (const listener of listeners) listener();
}

function hostOf(src: string): string {
  try {
    return new URL(src).host || "a web server";
  } catch {
    return "a web server";
  }
}

function RemoteImagePlaceholder({ src, alt, insecure, failed = false, onLoad }: {
  src: string; alt?: string; insecure: boolean; failed?: boolean; onLoad?: () => void;
}) {
  const host = hostOf(src);
  const label = insecure
    ? `Insecure remote image from ${host}, blocked`
    : failed ? `Remote image from ${host} could not be loaded`
      : onLoad ? `Remote image from ${host}` : `Remote image from ${host}, blocked`;
  return <span className="markdown-remote-image" role="img" aria-label={alt ? `${alt}: ${label}` : label} title={src}>
    {alt ? <span className="markdown-remote-image-alt">{alt}</span> : null}
    <span className="markdown-remote-image-note">{label}</span>
    {onLoad && !insecure ? <button type="button" onClick={onLoad}>Load remote images</button> : null}
  </span>;
}

function PreviewImage({ documentId, src, alt, loadImage, policy, approved }: {
  documentId: string; src?: string; alt?: string; loadImage?: ImageLoader; policy: RemoteImagePolicy; approved: boolean;
}) {
  const [resolvedSource, setResolvedSource] = useState<string | null>(null);
  // The remote source that failed to load, so a changed source gets a fresh try.
  const [failedSource, setFailedSource] = useState<string | null>(null);
  const remote = src !== undefined && /^https?:/i.test(src);

  useEffect(() => {
    if (!src || remote) return;
    if (/^data:/i.test(src)) {
      setResolvedSource(src);
      return;
    }
    let active = true;
    setResolvedSource(null);
    let relativePath: string;
    try {
      relativePath = decodeURIComponent(src.split(/[?#]/, 1)[0]);
    } catch {
      return;
    }
    void (loadImage ?? getMarkdownPreviewImage)(documentId, relativePath)
      .then((image) => { if (active) setResolvedSource(image); })
      .catch(() => { if (active) setResolvedSource(null); });
    return () => { active = false; };
  }, [documentId, loadImage, remote, src]);

  if (src && remote) {
    // Plain http is never fetched: the request and the image travel unencrypted.
    const insecure = /^http:/i.test(src);
    if (!insecure && (policy === "allow" || approved)) {
      // A server that is down, refuses, or sends no image would otherwise leave an empty gap.
      if (failedSource === src) return <RemoteImagePlaceholder src={src} alt={alt} insecure={false} failed />;
      return <img src={src} alt={alt ?? ""} onError={() => setFailedSource(src)} />;
    }
    return <RemoteImagePlaceholder src={src} alt={alt} insecure={insecure}
      onLoad={policy === "ask" ? () => approveRemoteImages(documentId) : undefined} />;
  }
  return resolvedSource ? <img src={resolvedSource} alt={alt ?? ""} /> : <span>{alt}</span>;
}

export function MarkdownPreview({ documentId, source, loadImage, className, remoteImages = DEFAULT_REMOTE_IMAGE_POLICY }: {
  documentId: string; source: string; loadImage?: ImageLoader; className?: string;
  /** What to do with `https:` images; `http:` images are always blocked. */
  remoteImages?: RemoteImagePolicy;
}) {
  const approved = useSyncExternalStore(subscribe, () => approvedDocuments.has(documentId));
  return <article className={className ? `markdown-preview ${className}` : "markdown-preview"} aria-label="Markdown preview">
    <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml
      components={{
        a: ({ href, children }) => <a href={href} target="_blank" rel="noopener noreferrer">{children}</a>,
        img: ({ src, alt }) => <PreviewImage documentId={documentId} src={src} alt={alt} loadImage={loadImage}
          policy={remoteImages} approved={approved} />,
      }}>
      {source}
    </ReactMarkdown>
  </article>;
}
