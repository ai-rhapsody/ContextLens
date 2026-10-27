import { Fragment, type ReactNode } from "react";

/** Inline `**bold**` and `code`, built as React nodes so model output is never injected as HTML. */
function inline(text: string): ReactNode[] {
  return text.split(/(\*\*[^*]+\*\*|`[^`]+`)/g).map((part, i) => {
    if (part.startsWith("**") && part.endsWith("**") && part.length > 4) return <strong key={i}>{part.slice(2, -2)}</strong>;
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2) return <code key={i}>{part.slice(1, -1)}</code>;
    return <Fragment key={i}>{part}</Fragment>;
  });
}

/** Paragraphs and `-` / `*` bullet lists: the only Markdown the tutor prompt asks for. */
export function Rich({ text }: { text: string }) {
  const blocks: ReactNode[] = [];
  let items: string[] = [];
  const flushList = () => {
    if (items.length) {
      const list = items;
      blocks.push(<ul key={`u${blocks.length}`}>{list.map((t, i) => <li key={i}>{inline(t)}</li>)}</ul>);
      items = [];
    }
  };
  for (const line of text.split("\n")) {
    const bullet = /^\s*[-*•]\s+(.*)$/.exec(line);
    if (bullet) items.push(bullet[1]);
    else {
      flushList();
      if (line.trim()) blocks.push(<p key={`p${blocks.length}`}>{inline(line)}</p>);
    }
  }
  flushList();
  return <>{blocks}</>;
}
