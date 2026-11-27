import * as React from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkBreaks from "remark-breaks";
import remarkGfm from "remark-gfm";
import { ExternalLink, Image as ImageIcon } from "lucide-react";

import { imageBasename } from "@/lib/markdown-images";
import { cn } from "@/lib/utils";

export type LocalImage = { name: string; url: string };

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
};

/**
 * The image's `src`: the project's local copy when its file name matches the
 * image's basename, otherwise the original http(s) URL.
 */
function resolveImageSrc(href: string, localImages?: LocalImage[]): string {
  if (!localImages || localImages.length === 0) return href;
  const key = imageBasename(href);
  const match = localImages.find((img) => imageBasename(img.name) === key);
  return match?.url ?? href;
}

/** The link chip an image falls back to when it can't be loaded. */
function ImageLink({ href, label }: { href: string; label: string }) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      className="inline-flex items-center gap-1 rounded border border-input px-1.5 py-0.5 align-middle text-xs text-muted-foreground hover:text-foreground"
    >
      <ImageIcon className="size-3" />
      {label}
      <ExternalLink className="size-3" />
    </a>
  );
}

/**
 * A Markdown image. The local copy is used when its file name matches, otherwise
 * the original http(s) URL is loaded directly. If the image fails to load, it
 * falls back to the link chip so there's still a way to open the original.
 */
function MarkdownImage({ src, alt, localImages }: { src?: string; alt?: string; localImages?: LocalImage[] }) {
  const [failed, setFailed] = React.useState(false);
  const safe = safeHref(src);
  const label = alt?.trim() || "image";
  // A new source gets a fresh chance to load.
  React.useEffect(() => setFailed(false), [src]);
  if (!safe) return <span className="text-dim">[{label}]</span>;
  const resolved = resolveImageSrc(safe, localImages);
  if (failed) return <ImageLink href={safe} label={label} />;
  return (
    <a href={resolved} target="_blank" rel="noopener noreferrer" className="my-3 block w-fit">
      <img
        src={resolved}
        alt={alt}
        loading="lazy"
        decoding="async"
        referrerPolicy="no-referrer"
        onError={() => setFailed(true)}
        className="max-h-[480px] max-w-full rounded-lg border object-contain"
      />
    </a>
  );
}

/**
 * `** bold **` → `**bold**`: spaces just inside the markers stop CommonMark making it bold.
 * Fenced blocks and inline code keep their asterisks as written.
 */
function normalizeBoldSpacing(markdown: string): string {
  const tighten = (text: string) =>
    text
      .replace(/\*\*[ \t]+([^*\n]+?)[ \t]*\*\*/g, "**$1**")
      .replace(/\*\*([^*\n]+?)[ \t]+\*\*/g, "**$1**");
  const out: string[] = [];
  let fence: string | null = null;
  for (const line of markdown.split("\n")) {
    const trimmed = line.trimStart();
    const marker = trimmed.startsWith("```") ? "```" : trimmed.startsWith("~~~") ? "~~~" : null;
    if (fence) {
      out.push(line);
      if (marker === fence) fence = null;
      continue;
    }
    if (marker) {
      fence = marker;
      out.push(line);
      continue;
    }
    out.push(line.split(/(`+[^`]*`+)/g).map((part, i) => (i % 2 === 1 ? part : tighten(part))).join(""));
  }
  return out.join("\n");
}

/**
 * Renders a description written in Markdown. Raw HTML is dropped,
 * single newlines are kept as line breaks so older plain-text descriptions look the same.
 * `localImages` maps file names to local URLs so images in the description are
 * loaded from the project's own copies when a match exists.
 */
export function Markdown({
  children,
  className,
  localImages,
}: {
  children: string;
  className?: string;
  localImages?: LocalImage[];
}) {
  return (
    <div className={cn("text-[15px] leading-relaxed break-words text-muted-foreground", className)}>
      <ReactMarkdown
        remarkPlugins={[remarkGfm, remarkBreaks]}
        components={{
          ...components,
          img: ({ src, alt }) => (
            <MarkdownImage src={typeof src === "string" ? src : undefined} alt={alt} localImages={localImages} />
          ),
        }}
        skipHtml
      >
        {normalizeBoldSpacing(children)}
      </ReactMarkdown>
    </div>
  );
}
