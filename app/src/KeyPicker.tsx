import { useEffect, useState } from "react";

// Combos use the same text form as the Rust side (src/keys.rs): canonical key
// names joined with "+", e.g. "ctrl+shift+a".

const MODIFIERS = ["ctrl", "alt", "shift", "meta"];
const MAX_COMBOS = 10;
const MAX_HOLD_MS = 600_000;
const isMac = navigator.userAgent.includes("Mac");

const CODE_NAMES: Record<string, string> = {
  Space: "space",
  Enter: "enter",
  NumpadEnter: "enter",
  Tab: "tab",
  Escape: "escape",
  Backspace: "backspace",
  Delete: "delete",
  CapsLock: "capslock",
  Home: "home",
  End: "end",
  PageUp: "pageup",
  PageDown: "pagedown",
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  ShiftLeft: "shift",
  ShiftRight: "shift",
  ControlLeft: "ctrl",
  ControlRight: "ctrl",
  AltLeft: "alt",
  AltRight: "alt",
  MetaLeft: "meta",
  MetaRight: "meta",
  OSLeft: "meta",
  OSRight: "meta",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backquote: "`",
};

/** Maps a physical key (KeyboardEvent.code) to its combo name, if supported. */
function codeToName(code: string): string | null {
  const letter = /^Key([A-Z])$/.exec(code);
  if (letter) return letter[1].toLowerCase();
  const digit = /^Digit(\d)$/.exec(code);
  if (digit) return digit[1];
  const fn = /^F(\d{1,2})$/.exec(code);
  if (fn && Number(fn[1]) >= 1 && Number(fn[1]) <= 20) return `f${fn[1]}`;
  return CODE_NAMES[code] ?? null;
}

const LABELS: Record<string, string> = {
  ctrl: "Ctrl",
  shift: "Shift",
  alt: isMac ? "Option" : "Alt",
  meta: isMac ? "Cmd" : "Super",
  space: "Space",
  enter: "Enter",
  tab: "Tab",
  escape: "Esc",
  backspace: "Backspace",
  delete: "Delete",
  capslock: "Caps Lock",
  home: "Home",
  end: "End",
  pageup: "Page Up",
  pagedown: "Page Down",
  up: "↑",
  down: "↓",
  left: "←",
  right: "→",
  plus: "+",
};

/** "ctrl+shift+a" → "Ctrl + Shift + A" */
export function formatCombo(combo: string): string {
  return combo
    .split("+")
    .map((k) => LABELS[k] ?? k.toUpperCase())
    .join(" + ");
}

export type TypeMode = "random" | "keys";

interface Props {
  mode: TypeMode;
  keys: string[];
  holdMs: number;
  disabled: boolean;
  onChange: (patch: { typeMode?: TypeMode; keys?: string[]; holdMs?: number }) => void;
}

export default function KeyPicker({ mode, keys, holdMs, disabled, onChange }: Props) {
  const [recording, setRecording] = useState(false);
  const [unsupported, setUnsupported] = useState<string | null>(null);

  useEffect(() => {
    if (disabled) setRecording(false);
  }, [disabled]);

  // While recording, capture keys globally so nothing else reacts to them
  // (Tab moving focus, F5 reloading, …).
  useEffect(() => {
    if (!recording) return;
    const held = new Set<string>();
    const heldMods = () => MODIFIERS.filter((m) => held.has(m));

    const commit = (combo: string) => {
      setRecording(false);
      if (!keys.includes(combo) && keys.length < MAX_COMBOS) onChange({ keys: [...keys, combo] });
    };
    const onDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.repeat) return;
      const name = codeToName(e.code);
      if (!name) {
        setUnsupported(e.key && e.key !== "Unidentified" ? e.key : e.code);
        return;
      }
      setUnsupported(null);
      if (MODIFIERS.includes(name)) held.add(name);
      else commit([...heldMods(), name].join("+"));
    };
    // A modifier released without another key: record the modifiers alone.
    const onUp = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const name = codeToName(e.code);
      if (name && MODIFIERS.includes(name) && held.size) commit(heldMods().join("+"));
    };
    const cancel = () => setRecording(false);

    window.addEventListener("keydown", onDown, true);
    window.addEventListener("keyup", onUp, true);
    window.addEventListener("blur", cancel);
    return () => {
      window.removeEventListener("keydown", onDown, true);
      window.removeEventListener("keyup", onUp, true);
      window.removeEventListener("blur", cancel);
    };
  }, [recording, keys, onChange]);

  return (
    <div className="keypicker">
      <div className="segmented" role="radiogroup" aria-label="What to type">
        {(["random", "keys"] as const).map((m) => (
          <button
            key={m}
            role="radio"
            aria-checked={mode === m}
            className={mode === m ? "selected" : undefined}
            disabled={disabled}
            onClick={() => onChange({ typeMode: m })}
          >
            {m === "random" ? "Random letter" : "Specific keys"}
          </button>
        ))}
      </div>

      {mode === "keys" && (
        <>
          <div className="chips">
            {keys.map((k) => (
              <span key={k} className="chip">
                <kbd>{formatCombo(k)}</kbd>
                <button
                  className="remove"
                  aria-label={`Remove ${formatCombo(k)}`}
                  disabled={disabled}
                  onClick={() => onChange({ keys: keys.filter((x) => x !== k) })}
                >
                  ×
                </button>
              </span>
            ))}
            {keys.length < MAX_COMBOS && (
              <button
                className={recording ? "chip add recording" : "chip add"}
                disabled={disabled}
                onClick={() => {
                  setUnsupported(null);
                  setRecording((r) => !r);
                }}
              >
                {recording ? "Press a key or combo…" : "+ Record key"}
              </button>
            )}
          </div>
          <p className="note">
            {unsupported
              ? `"${unsupported}" isn't supported. Try a letter, number, F1–F20, arrow, or modifier combo.`
              : recording
                ? "Click again to cancel."
                : keys.length > 1
                  ? "Picks one at random each time. Keys go to the focused window."
                  : "Keys go to whichever window is focused."}
          </p>
          <label className="inline">
            Hold for
            <input
              type="number"
              min={0}
              max={MAX_HOLD_MS}
              step={10}
              value={holdMs}
              disabled={disabled}
              onChange={(e) => onChange({ holdMs: e.target.valueAsNumber || 0 })}
            />
            ms
          </label>
        </>
      )}
    </div>
  );
}

export function isHoldValid(ms: number): boolean {
  return Number.isInteger(ms) && ms >= 0 && ms <= MAX_HOLD_MS;
}
