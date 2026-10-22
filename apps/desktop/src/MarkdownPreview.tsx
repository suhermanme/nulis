import { useEffect, useState } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { getMarkdownPreviewImage } from "./ipc";

/** Reads an image relative to the document as a data URL. */
export type ImageLoader = (documentId: string, relativePath: string) => Promise<string>;

function PreviewImage({ documentId, src, alt, loadImage }: { documentId: string; src?: string; alt?: string; loadImage?: ImageLoader }) {
  const [resolvedSource, setResolvedSource] = useState<string | null>(null);

  useEffect(() => {
    if (!src) return;
    if (/^(https?:|data:)/i.test(src)) {
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
  }, [documentId, loadImage, src]);

  return resolvedSource ? <img src={resolvedSource} alt={alt ?? ""} /> : <span>{alt}</span>;
}

export function MarkdownPreview({ documentId, source, loadImage, className }: {
  documentId: string; source: string; loadImage?: ImageLoader; className?: string;
}) {
  return <article className={className ? `markdown-preview ${className}` : "markdown-preview"} aria-label="Markdown preview">
    <ReactMarkdown remarkPlugins={[remarkGfm]} skipHtml
      components={{
        a: ({ href, children }) => <a href={href} target="_blank" rel="noopener noreferrer">{children}</a>,
        img: ({ src, alt }) => <PreviewImage documentId={documentId} src={src} alt={alt} loadImage={loadImage} />,
      }}>
      {source}
    </ReactMarkdown>
  </article>;
}
