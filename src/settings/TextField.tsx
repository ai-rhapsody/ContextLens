import { useEffect, useState } from "react";

/** Keeps a local draft and commits on blur or Enter, so typing never triggers a save per keystroke. */
export function TextField(props: {
  value: string;
  onCommit: (v: string) => void;
  placeholder?: string;
  list?: string;
  className?: string;
  ariaLabel?: string;
}) {
  const [draft, setDraft] = useState(props.value);
  useEffect(() => setDraft(props.value), [props.value]);
  const commit = () => {
    const v = draft.trim();
    if (v !== props.value) props.onCommit(v);
    else setDraft(props.value);
  };
  return (
    <input
      className={`field ${props.className ?? ""}`}
      value={draft}
      list={props.list}
      placeholder={props.placeholder}
      aria-label={props.ariaLabel}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && (e.target as HTMLInputElement).blur()}
      spellCheck={false}
    />
  );
}
