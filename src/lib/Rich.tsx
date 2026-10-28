import { Fragment, type ReactNode } from "react";

/** Inline `**bold**`, `*italic*` / `_italic_` and `code`, built as React nodes so model output
 *  is never injected as HTML. */
export function inline(text: string): ReactNode[] {
  return text.split(/(\*\*[^*]+\*\*|`[^`]+`|(?<![\w*])\*[^*\s][^*]*\*(?![\w*])|(?<!\w)_[^_\s][^_]*_(?!\w))/g).map((part, i) => {
    if (part.startsWith("**") && part.endsWith("**") && part.length > 4) return <strong key={i}>{part.slice(2, -2)}</strong>;
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2) return <code key={i}>{part.slice(1, -1)}</code>;
    if (/^([*_]).+\1$/.test(part) && part.length > 2) return <em key={i}>{part.slice(1, -1)}</em>;
    return <Fragment key={i}>{part}</Fragment>;
  });
}

/** Paragraphs, `-` / `*` bullet lists, `1.` numbered lists and `###` subheadings: what models
 *  write even when asked for less. */
export function Rich({ text }: { text: string }) {
  const blocks: ReactNode[] = [];
  let items: string[] = [];
  let ordered = false;
  const flushList = () => {
    if (items.length) {
      const list = items;
      const children = list.map((t, i) => <li key={i}>{inline(t)}</li>);
      blocks.push(ordered ? <ol key={`o${blocks.length}`}>{children}</ol> : <ul key={`u${blocks.length}`}>{children}</ul>);
      items = [];
    }
  };
  for (const line of text.split("\n")) {
    const bullet = /^\s*[-*•]\s+(.*)$/.exec(line);
    const numbered = /^\s*\d+[.)]\s+(.*)$/.exec(line);
    const heading = /^\s*#{1,6}\s+(.*)$/.exec(line);
    if (bullet || numbered) {
      const isOrdered = !!numbered;
      if (items.length && isOrdered !== ordered) flushList();
      ordered = isOrdered;
      items.push((bullet ?? numbered)![1]);
    } else {
      flushList();
      if (heading) blocks.push(<h3 key={`h${blocks.length}`}>{inline(heading[1].replace(/\*\*/g, ""))}</h3>);
      else if (/^\s*([-*_])\1{2,}\s*$/.test(line)) continue; // a horizontal rule
      else if (line.trim()) blocks.push(<p key={`p${blocks.length}`}>{inline(line)}</p>);
    }
  }
  flushList();
  return <>{blocks}</>;
}
