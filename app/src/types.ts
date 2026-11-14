// Mirrors the serde types in src/lib.rs.
export type Action = "move" | "click" | "type" | "keepAwake";

export interface JobConfig {
  action: Action;
  intervalSecs: number;
  /** null = run until stopped */
  count: number | null;
  keepDisplayOn?: boolean;
  /** type only: combos like "ctrl+shift+a"; empty/absent = random letter */
  keys?: string[];
  holdMs?: number;
}

export type JobEvent =
  | { kind: "started"; action: Action; intervalSecs: number; count: number | null }
  | { kind: "performed"; iteration: number; count: number | null; detail: string }
  | { kind: "failed"; iteration: number; count: number | null; error: string }
  | { kind: "finished"; completed: number }
  | { kind: "stopped"; completed: number };
