export interface Section {
  title: string | null;
  body: string;
}

/**
 * Splits streamed tutor output on `## ` headings. Safe to call on every delta:
 * a half-received heading line is held back until its newline arrives.
 */
export function parseSections(raw: string): Section[] {
  const lines = raw.split("\n");
  const incomplete = raw.endsWith("\n") ? "" : lines.pop() ?? "";
  const sections: Section[] = [];
  let current: Section = { title: null, body: "" };
  const flush = () => {
    if (current.title !== null || current.body.trim()) sections.push(current);
  };
  for (const line of lines) {
    const m = /^##\s+(.+?)\s*$/.exec(line);
    if (m) {
      flush();
      current = { title: m[1], body: "" };
    } else {
      current.body += line + "\n";
    }
  }
  if (incomplete && !incomplete.startsWith("#")) current.body += incomplete;
  flush();
  return sections.map((s) => ({ ...s, body: s.body.trim() }));
}
