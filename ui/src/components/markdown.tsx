import ReactMarkdown, { type Components } from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import { ExternalLink, Image as ImageIcon } from "lucide-react";

import { cn } from "@/lib/utils";

/** Only http(s) and mailto links are followed; anything else renders as plain text. */
function safeHref(href: string | undefined): string | null {
  if (!href) return null;
  try {
    const url = new URL(href, window.location.origin);
    return ["http:", "https:", "mailto:"].includes(url.protocol) ? url.href : null;
  } catch {
    return null;
  }
}

const components: Components = {
  h1: ({ children }) => <h3 className="font-display mt-5 mb-2 text-xl font-semibold text-foreground first:mt-0">{children}</h3>,
  h2: ({ children }) => <h3 className="font-display mt-5 mb-2 text-lg font-semibold text-foreground first:mt-0">{children}</h3>,
  h3: ({ children }) => <h4 className="font-display mt-4 mb-1.5 text-base font-semibold text-foreground first:mt-0">{children}</h4>,
  h4: ({ children }) => <h5 className="mt-4 mb-1 font-semibold text-foreground first:mt-0">{children}</h5>,
  h5: ({ children }) => <h6 className="mt-3 mb-1 font-semibold text-foreground first:mt-0">{children}</h6>,
  h6: ({ children }) => <h6 className="mt-3 mb-1 font-semibold text-foreground first:mt-0">{children}</h6>,
  p: ({ children }) => <p className="my-2.5 first:mt-0 last:mb-0">{children}</p>,
  strong: ({ children }) => <strong className="font-semibold text-foreground">{children}</strong>,
  ul: ({ children }) => <ul className="my-2.5 list-disc space-y-1 pl-5 marker:text-dim">{children}</ul>,
  ol: ({ children }) => <ol className="my-2.5 list-decimal space-y-1 pl-5 marker:text-dim">{children}</ol>,
  blockquote: ({ children }) => <blockquote className="my-3 border-l-2 border-line-strong pl-3 italic">{children}</blockquote>,
  hr: () => <hr className="my-4 border-border" />,
  code: ({ children, className }) => (
    <code className={cn("rounded bg-secondary px-1 py-0.5 font-mono text-[0.85em] text-foreground", className)}>{children}</code>
  ),
  pre: ({ children }) => <pre className="my-3 overflow-x-auto rounded-lg bg-secondary p-3 [&_code]:bg-transparent [&_code]:p-0">{children}</pre>,
  table: ({ children }) => (
    <div className="my-3 overflow-x-auto">
      <table className="w-full border-collapse text-sm">{children}</table>
    </div>
  ),
  th: ({ children }) => <th className="border-b px-2 py-1 text-left font-semibold text-foreground">{children}</th>,
  td: ({ children }) => <td className="border-b px-2 py-1">{children}</td>,
  a: ({ href, children }) => {
    const safe = safeHref(href);
    if (!safe) return <span>{children}</span>;
    return (
      <a href={safe} target="_blank" rel="noopener noreferrer" className="text-signal underline-offset-4 hover:underline">
        {children}
      </a>
    );
  },
  // Remote images aren't loaded (no hotlinking or tracking); they become links.
  img: ({ src, alt }) => {
    const safe = safeHref(typeof src === "string" ? src : undefined);
    const label = alt?.trim() || "image";
    if (!safe) return <span className="text-dim">[{label}]</span>;
    return (
      <a
        href={safe}
        target="_blank"
        rel="noopener noreferrer"
        className="inline-flex items-center gap-1 rounded border border-input px-1.5 py-0.5 align-middle text-xs text-muted-foreground hover:text-foreground"
      >
        <ImageIcon className="size-3" />
        {label}
        <ExternalLink className="size-3" />
      </a>
    );
  },
};

/**
 * Renders a description written in Markdown. Raw HTML is dropped,
 * single newlines are kept as line breaks so older plain-text descriptions look the same.
 */
export function Markdown({ children, className }: { children: string; className?: string }) {
  return (
    <div className={cn("text-[15px] leading-relaxed break-words text-muted-foreground", className)}>
      <ReactMarkdown remarkPlugins={[remarkGfm, remarkBreaks]} components={components} skipHtml>
        {children}
      </ReactMarkdown>
    </div>
  );
}
