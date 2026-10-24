import { useId, type ComponentPropsWithoutRef } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { Link } from "react-router";
import { resolveWikiLink } from "./wiki-types";
import { useReducedMotion } from "../ui/use-reduced-motion";
import { remarkWikiOutline } from "./wiki-outline";

/** wiki の Markdown を描画する。相対リンクは SPA 内遷移、脚注はページ内スクロールにする (HashRouter のため)。 */
export function WikiMarkdown({ body, from, onOpen, outlineLabel }: { body: string; from: string; onOpen: (path: string) => void; outlineLabel?: string }) {
  const idPrefix = `wiki-${useId()}-`;
  const reducedMotion = useReducedMotion();
  return (
    <div className="text-sm leading-relaxed space-y-3 [&_.footnotes]:mt-8 [&_.footnotes]:pt-3 [&_.footnotes]:border-t [&_.footnotes]:border-border [&_.footnotes]:text-xs [&_.footnotes]:text-muted-foreground">
      <Markdown
        remarkPlugins={outlineLabel ? [remarkGfm, [remarkWikiOutline, { idPrefix, label: outlineLabel }]] : [remarkGfm]}
        skipHtml
        remarkRehypeOptions={{ clobberPrefix: idPrefix, footnoteLabel: "出典", footnoteBackLabel: "本文へ戻る", footnoteLabelProperties: {} }}
        components={{
          // タイトルはヘッダで表示済み。
          h1: () => null,
          h2: ({ children, id }) => <h2 id={id === "footnote-label" ? `${idPrefix}footnote-label` : id} tabIndex={id?.startsWith(`${idPrefix}section-`) ? -1 : undefined} className="text-base font-semibold mt-6 mb-2 scroll-mt-6">{children}</h2>,
          h3: ({ children, id }) => <h3 id={id} tabIndex={id?.startsWith(`${idPrefix}section-`) ? -1 : undefined} className="text-sm font-semibold mt-4 mb-1 scroll-mt-6">{children}</h3>,
          nav: ({ children, "aria-label": label }) => <nav aria-label={label} className="rounded-md border border-border bg-muted/30 p-4 space-y-2 break-words">{children}</nav>,
          h4: ({ children }) => <h4 className="text-sm font-medium text-muted-foreground mt-3 mb-1">{children}</h4>,
          ul: ({ children }) => <ul className="list-disc pl-5 space-y-1.5">{children}</ul>,
          ol: ({ children }) => <ol className="list-decimal pl-5 space-y-1">{children}</ol>,
          table: ({ children }) => (
            // 日付・種別などの短い列が 1 文字ずつ折り返されないようにする。
            <div className="overflow-x-auto border border-border rounded-md [&_td:first-child]:whitespace-nowrap [&_td]:min-w-[3.5em]">
              <table className="w-full text-xs">{children}</table>
            </div>
          ),
          th: ({ children }) => <th className="text-left font-medium bg-muted/50 px-2 py-1.5 border-b border-border">{children}</th>,
          td: ({ children }) => <td className="px-2 py-1.5 border-b border-border align-top">{children}</td>,
          pre: ({ children }) => (
            <pre className="whitespace-pre-wrap break-all text-xs leading-relaxed bg-muted/50 border border-border rounded-md p-3 max-h-96 overflow-y-auto [&_code]:bg-transparent [&_code]:p-0">{children}</pre>
          ),
          code: ({ children }) => <code className="px-1 py-0.5 rounded bg-muted text-[0.9em]">{children}</code>,
          a: ({ node: _node, ...props }) => <WikiLink {...props} aria-describedby={props["aria-describedby"] === "footnote-label" ? `${idPrefix}footnote-label` : props["aria-describedby"]} from={from} onOpen={onOpen} reducedMotion={reducedMotion} />,
        }}
      >
        {body}
      </Markdown>
    </div>
  );
}

function WikiLink({ href = "", from, onOpen, reducedMotion, children, ...props }: ComponentPropsWithoutRef<"a"> & { from: string; onOpen: (path: string) => void; reducedMotion: boolean }) {
  if (/^#\/hanrei\/\d+\/laws\/[A-Za-z0-9]+\/\d+(?:_\d+)*(?:\?[^#]*)?$/.test(href)) {
    return <Link {...props} to={href.slice(1)} className="text-primary hover:underline">{children}</Link>;
  }
  if (/^[a-z][a-z\d+.-]*:/i.test(href) || href.startsWith("//")) {
    return <a {...props} href={href} target="_blank" rel="noreferrer" className="text-primary hover:underline">{children}</a>;
  }
  if (href.startsWith("#")) {
    return (
      <a
        {...props}
        href={href}
        className="text-primary hover:underline"
        onClick={e => {
          e.preventDefault();
          let id: string;
          try { id = decodeURIComponent(href.slice(1)); } catch { return; }
          const target = document.getElementById(id);
          const heading = target?.matches('h2[tabindex="-1"], h3[tabindex="-1"]');
          if (heading) target?.focus({ preventScroll: true });
          target?.scrollIntoView({ behavior: reducedMotion ? "instant" : "smooth", block: heading ? "start" : "center" });
        }}
      >
        {children}
      </a>
    );
  }
  const target = resolveWikiLink(from, href);
  return (
    <a
      {...props}
      href={target ? `#/wiki/${target}` : href}
      className="text-primary hover:underline"
      onClick={e => {
        if (!target || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
        e.preventDefault();
        onOpen(target);
      }}
    >
      {children}
    </a>
  );
}
