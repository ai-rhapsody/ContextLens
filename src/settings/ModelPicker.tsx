import { useEffect, useState } from "react";
import type { ModelOption } from "../lib/models";
import { TextField } from "./TextField";

const CUSTOM = "__custom__";

/** A preset list with "自訂…" at the end; a name not in the list shows as custom. */
export function ModelPicker(props: {
  value: string;
  presets: ModelOption[];
  onCommit: (v: string) => void;
  ariaLabel: string;
  placeholder?: string;
  /** The preset marked （預設）; the first one if not given. */
  defaultId?: string;
}) {
  const known = props.presets.some((p) => p.id === props.value);
  const [custom, setCustom] = useState(!known);
  useEffect(() => setCustom(!props.presets.some((p) => p.id === props.value)), [props.value, props.presets]);

  return (
    <div className="model-picker">
      <select
        className="field" aria-label={props.ariaLabel}
        value={custom ? CUSTOM : props.value}
        onChange={(e) => {
          if (e.target.value === CUSTOM) setCustom(true);
          else {
            setCustom(false);
            props.onCommit(e.target.value);
          }
        }}
      >
        {props.presets.map((p, i) => (
          <option key={p.id} value={p.id}>
            {p.label}{p.note ? `　${p.note}` : ""}{(props.defaultId === undefined ? i === 0 : p.id === props.defaultId) ? "（預設）" : ""}
          </option>
        ))}
        <option value={CUSTOM}>自訂…</option>
      </select>
      {custom && (
        <TextField
          value={known ? "" : props.value}
          ariaLabel={`${props.ariaLabel}（自訂）`}
          placeholder={props.placeholder ?? "輸入模型名稱"}
          onCommit={(v) => v && props.onCommit(v)}
        />
      )}
    </div>
  );
}
