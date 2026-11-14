import { type ReactNode, useCallback, useEffect, useReducer, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { Action, JobConfig, JobEvent } from "./types";
import KeyPicker, { formatCombo, isHoldValid, type TypeMode } from "./KeyPicker";

const ACTIONS: { id: Action; label: string; hint: string; icon: ReactNode }[] = [
  {
    id: "move",
    label: "Move",
    hint: "Nudge the cursor",
    icon: (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M12 2v20M2 12h20M12 2l-3 3M12 2l3 3M12 22l-3-3M12 22l3-3M2 12l3-3M2 12l3 3M22 12l-3-3M22 12l-3 3" />
      </svg>
    ),
  },
  {
    id: "click",
    label: "Click",
    hint: "Left click in place",
    icon: (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M5 3l6 16 2.5-6.5L20 10z" />
        <path d="M14 14l5 5" />
      </svg>
    ),
  },
  {
    id: "type",
    label: "Type",
    hint: "Random letter or your keys",
    icon: (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <rect x="2" y="6" width="20" height="12" rx="2" />
        <path d="M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10" />
      </svg>
    ),
  },
  {
    id: "keepAwake",
    label: "Keep awake",
    hint: "Block sleep, no input",
    icon: (
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M4 10h12v4a5 5 0 0 1-5 5H9a5 5 0 0 1-5-5z" />
        <path d="M16 11.5h1.5a2.5 2.5 0 0 1 0 5H16" />
        <path d="M8 3.5v3M12 3.5v3" />
      </svg>
    ),
  },
];

const VERB: Record<Action, string> = { move: "move", click: "click", type: "keypress", keepAwake: "check" };
const SETTINGS_KEY = "automau5.settings";
const MAX_LOG = 100;
/** How often a keep-awake job reports progress. */
const AWAKE_TICK_SECS = 60;

interface Settings {
  action: Action;
  // Move / click / type
  interval: number;
  unit: "sec" | "min";
  count: number;
  forever: boolean;
  // Type
  typeMode: TypeMode;
  keys: string[];
  holdMs: number;
  // Keep awake
  awakeFor: number;
  awakeUnit: "min" | "hour";
  awakeForever: boolean;
  keepDisplayOn: boolean;
}

const DEFAULT_SETTINGS: Settings = {
  action: "move",
  interval: 60,
  unit: "sec",
  count: 60,
  forever: false,
  typeMode: "random",
  keys: [],
  holdMs: 100,
  awakeFor: 2,
  awakeUnit: "hour",
  awakeForever: false,
  keepDisplayOn: false,
};

function loadSettings(): Settings {
  try {
    const raw = localStorage.getItem(SETTINGS_KEY);
    if (raw) {
      const saved = { ...DEFAULT_SETTINGS, ...JSON.parse(raw) };
      if (ACTIONS.some((a) => a.id === saved.action)) return saved;
    }
  } catch {
    /* storage unavailable or corrupt */
  }
  return DEFAULT_SETTINGS;
}

function toConfig(s: Settings): JobConfig {
  if (s.action === "keepAwake") {
    return {
      action: s.action,
      intervalSecs: AWAKE_TICK_SECS,
      count: s.awakeForever ? null : Math.round(s.awakeFor * (s.awakeUnit === "hour" ? 60 : 1)),
      keepDisplayOn: s.keepDisplayOn,
    };
  }
  const useKeys = s.action === "type" && s.typeMode === "keys";
  return {
    action: s.action,
    intervalSecs: Math.round(s.interval * (s.unit === "min" ? 60 : 1)),
    count: s.forever ? null : s.count,
    ...(useKeys && { keys: s.keys, holdMs: s.holdMs }),
  };
}

function validationError(s: Settings, c: JobConfig): string | null {
  if (c.intervalSecs < 1 || (c.count !== null && !(Number.isInteger(c.count) && c.count >= 1))) {
    return "Values must be at least 1.";
  }
  if (s.action === "type" && s.typeMode === "keys") {
    if (s.keys.length === 0) return "Record at least one key.";
    if (!isHoldValid(s.holdMs)) return "Hold must be 0–600000 ms.";
  }
  return null;
}

interface LogEntry {
  id: number;
  time: Date;
  text: string;
  error?: boolean;
}

interface JobState {
  status: "idle" | "starting" | "running" | "stopping";
  config: JobConfig | null;
  done: number;
  startedAt: number | null;
  nextAt: number | null;
  log: LogEntry[];
  error: string | null;
}

type Msg =
  | { type: "event"; event: JobEvent }
  | { type: "start"; config: JobConfig }
  | { type: "startFailed"; error: string }
  | { type: "stop" }
  | { type: "restore"; config: JobConfig }
  | { type: "dismiss" };

let logId = 0;
function addLog(log: LogEntry[], text: string, error = false): LogEntry[] {
  return [{ id: logId++, time: new Date(), text, error }, ...log].slice(0, MAX_LOG);
}

function startedText(c: JobConfig): string {
  if (c.action === "keepAwake") {
    const span = c.count === null ? "until stopped" : `for ${formatDuration(c.count * c.intervalSecs)}`;
    return `Keeping awake ${span}${c.keepDisplayOn ? " (display on)" : ""}`;
  }
  const what = c.keys?.length
    ? c.keys.length === 1
      ? formatCombo(c.keys[0])
      : `one of ${c.keys.map(formatCombo).join(", ")}`
    : c.action;
  return `Started: ${c.count ?? "∞"} × ${what} every ${formatDuration(c.intervalSecs)}`;
}

function progressText(iteration: number, count: number | null): string {
  return count === null ? `#${iteration}` : `${iteration}/${count}`;
}

function reducer(s: JobState, m: Msg): JobState {
  const now = Date.now();
  switch (m.type) {
    case "start":
      // Remember the full config: events don't carry keepDisplayOn.
      return { ...s, status: "starting", config: m.config, error: null };
    case "startFailed":
      return { ...s, status: "idle", error: m.error };
    case "stop":
      return { ...s, status: "stopping" };
    case "dismiss":
      return { ...s, error: null };
    case "restore":
      return { ...s, status: "running", config: m.config, startedAt: null, nextAt: null };
    case "event": {
      const e = m.event;
      const intervalMs = (s.config?.intervalSecs ?? 0) * 1000;
      switch (e.kind) {
        case "started": {
          const config: JobConfig = {
            ...s.config,
            action: e.action,
            intervalSecs: e.intervalSecs,
            count: e.count,
          };
          return {
            ...s,
            status: s.status === "stopping" ? "stopping" : "running",
            config,
            done: 0,
            startedAt: now,
            nextAt: now + e.intervalSecs * 1000,
            log: addLog(s.log, startedText(config)),
          };
        }
        case "performed":
        case "failed": {
          const quiet = e.kind === "performed" && s.config?.action === "keepAwake";
          return {
            ...s,
            done: e.iteration,
            nextAt: e.count === null || e.iteration < e.count ? now + intervalMs : null,
            log: quiet
              ? s.log
              : e.kind === "performed"
                ? addLog(s.log, `${progressText(e.iteration, e.count)} · ${e.detail}`)
                : addLog(s.log, `${progressText(e.iteration, e.count)} · ${e.error}`, true),
          };
        }
        case "finished":
          return {
            ...s,
            status: "idle",
            nextAt: null,
            log: addLog(s.log, s.config?.action === "keepAwake" ? "Keep awake finished" : `Finished all ${e.completed}`),
          };
        case "stopped":
          return {
            ...s,
            status: "idle",
            nextAt: null,
            log: addLog(
              s.log,
              s.config?.action === "keepAwake" || s.config?.count === null
                ? `Stopped after ${formatDuration(s.startedAt ? (now - s.startedAt) / 1000 : 0)}`
                : `Stopped after ${e.completed}`,
            ),
          };
      }
    }
  }
}

function formatDuration(totalSecs: number): string {
  const s = Math.max(0, Math.round(totalSecs));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h) return m ? `${h} h ${m} min` : `${h} h`;
  if (m) return sec ? `${m} min ${sec} s` : `${m} min`;
  return `${sec} s`;
}

function formatClock(ms: number): string {
  const s = Math.max(0, Math.ceil(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

function endTime(secsFromNow: number): string {
  return new Date(Date.now() + secsFromNow * 1000).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

const isMac = navigator.userAgent.includes("Mac");

export default function App() {
  const [settings, setSettings] = useState<Settings>(loadSettings);
  const [job, dispatch] = useReducer(reducer, {
    status: "idle",
    config: null,
    done: 0,
    startedAt: null,
    nextAt: null,
    log: [],
    error: null,
  });
  const [now, setNow] = useState(Date.now());

  const busy = job.status !== "idle";
  const keepAwake = settings.action === "keepAwake";
  const draft = toConfig(settings);
  const invalid = validationError(settings, draft);
  const valid = invalid === null;

  useEffect(() => {
    try {
      localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings));
    } catch {
      /* ignore */
    }
  }, [settings]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    listen<JobEvent>("job-event", (e) => dispatch({ type: "event", event: e.payload })).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    invoke<JobConfig | null>("job_status").then((config) => {
      if (!cancelled && config) {
        dispatch({ type: "restore", config });
        setSettings((s) => ({ ...s, action: config.action }));
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  useEffect(() => {
    if (!busy) return;
    const t = setInterval(() => setNow(Date.now()), 250);
    return () => clearInterval(t);
  }, [busy]);

  async function start() {
    dispatch({ type: "start", config: draft });
    try {
      await invoke("start_job", { config: draft });
    } catch (err) {
      dispatch({ type: "startFailed", error: String(err) });
    }
  }

  async function stop() {
    dispatch({ type: "stop" });
    await invoke("stop_job");
  }

  const update = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    setSettings((s) => ({ ...s, [key]: value }));
  const patchSettings = useCallback((patch: Partial<Settings>) => setSettings((s) => ({ ...s, ...patch })), []);

  const running = busy ? job.config : null;
  // Time until the next tick, plus a full interval for each one after it.
  const remainingSecs =
    running && running.count !== null
      ? job.nextAt
        ? (job.nextAt - now) / 1000 + (running.count - job.done - 1) * running.intervalSecs
        : (running.count - job.done) * running.intervalSecs
      : null;
  const draftTotal = draft.count === null ? null : draft.count * draft.intervalSecs;

  let summary: ReactNode;
  if (!valid) {
    summary = <span className="invalid">{invalid}</span>;
  } else if (running) {
    summary = remainingSecs !== null ? `About ${formatDuration(remainingSecs)} left` : "Runs until you press Stop";
  } else if (draftTotal === null) {
    summary = keepAwake ? "Prevents sleep until you press Stop" : "Runs until you press Stop";
  } else {
    summary = `${keepAwake ? "Prevents sleep for" : "Runs for"} ${formatDuration(draftTotal)} · until about ${endTime(draftTotal)}`;
  }

  let statusLeft = "";
  let statusRight = "";
  if (running) {
    if (running.action === "keepAwake") {
      statusLeft = running.keepDisplayOn ? "Awake · display on" : "Awake";
      statusRight =
        remainingSecs !== null
          ? `Ends in ${formatClock(remainingSecs * 1000)}`
          : job.startedAt
            ? `On for ${formatClock(now - job.startedAt)}`
            : "";
    } else {
      statusLeft = running.count === null ? `${job.done} done` : `${job.done} of ${running.count}`;
      statusRight = job.nextAt ? `Next ${VERB[running.action]} in ${formatClock(job.nextAt - now)}` : "";
    }
    if (job.status === "starting") statusRight = "Starting…";
    if (job.status === "stopping") statusRight = "Stopping…";
  }

  return (
    <main>
      <header>
        <h1>Automau5</h1>
        <p>Move, click, type, or just stay awake.</p>
      </header>

      {job.error && (
        <div className="banner" role="alert">
          <div>
            <strong>Couldn't start.</strong> {job.error}
            {isMac && !keepAwake && (
              <p>
                Allow Automau5 in System Settings → Privacy &amp; Security → Accessibility, then press
                Start again. Keep awake works without it.
              </p>
            )}
          </div>
          <button className="ghost" onClick={() => dispatch({ type: "dismiss" })} aria-label="Dismiss">
            ×
          </button>
        </div>
      )}

      <section className="card">
        <div className="actions" role="radiogroup" aria-label="Action">
          {ACTIONS.map((a) => (
            <button
              key={a.id}
              role="radio"
              aria-checked={settings.action === a.id}
              className={settings.action === a.id ? "action selected" : "action"}
              disabled={busy}
              onClick={() => update("action", a.id)}
            >
              {a.icon}
              <span className="label">{a.label}</span>
              <span className="hint">{a.hint}</span>
            </button>
          ))}
        </div>

        {keepAwake ? (
          <>
            <div className="fields">
              <label>
                <span>For</span>
                <div className="combo">
                  <input
                    type="number"
                    min={1}
                    value={settings.awakeFor}
                    disabled={busy || settings.awakeForever}
                    onChange={(e) => update("awakeFor", e.target.valueAsNumber || 0)}
                  />
                  <select
                    value={settings.awakeUnit}
                    disabled={busy || settings.awakeForever}
                    onChange={(e) => update("awakeUnit", e.target.value as Settings["awakeUnit"])}
                  >
                    <option value="min">minutes</option>
                    <option value="hour">hours</option>
                  </select>
                </div>
              </label>
            </div>
            <div className="checks">
              <label className="check">
                <input
                  type="checkbox"
                  checked={settings.awakeForever}
                  disabled={busy}
                  onChange={(e) => update("awakeForever", e.target.checked)}
                />
                Until I stop it
              </label>
              <label className="check">
                <input
                  type="checkbox"
                  checked={settings.keepDisplayOn}
                  disabled={busy}
                  onChange={(e) => update("keepDisplayOn", e.target.checked)}
                />
                Keep display on
              </label>
            </div>
          </>
        ) : (
          <>
            {settings.action === "type" && (
              <KeyPicker
                mode={settings.typeMode}
                keys={settings.keys}
                holdMs={settings.holdMs}
                disabled={busy}
                onChange={patchSettings}
              />
            )}
            <div className="fields">
              <label>
                <span>Every</span>
                <div className="combo">
                  <input
                    type="number"
                    min={1}
                    value={settings.interval}
                    disabled={busy}
                    onChange={(e) => update("interval", e.target.valueAsNumber || 0)}
                  />
                  <select
                    value={settings.unit}
                    disabled={busy}
                    onChange={(e) => update("unit", e.target.value as Settings["unit"])}
                  >
                    <option value="sec">seconds</option>
                    <option value="min">minutes</option>
                  </select>
                </div>
              </label>
              <label>
                <span>Times</span>
                <input
                  type="number"
                  min={1}
                  step={1}
                  value={settings.count}
                  disabled={busy || settings.forever}
                  onChange={(e) => update("count", e.target.valueAsNumber || 0)}
                />
              </label>
            </div>
            <div className="checks">
              <label className="check">
                <input
                  type="checkbox"
                  checked={settings.forever}
                  disabled={busy}
                  onChange={(e) => update("forever", e.target.checked)}
                />
                Until I stop it
              </label>
            </div>
          </>
        )}

        <p className="summary">{summary}</p>

        {busy ? (
          <button className="primary stop" onClick={stop} disabled={job.status === "stopping"}>
            {job.status === "stopping" ? "Stopping…" : "Stop"}
          </button>
        ) : (
          <button className="primary" onClick={start} disabled={!valid}>
            Start
          </button>
        )}
      </section>

      {running && (
        <section className="card status" aria-live="polite">
          <div className="status-row">
            <span>{statusLeft}</span>
            <span>{statusRight}</span>
          </div>
          {running.count !== null ? (
            <div
              className="bar"
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={running.count}
              aria-valuenow={job.done}
            >
              <div style={{ width: `${(job.done / running.count) * 100}%` }} />
            </div>
          ) : (
            <div className="bar indeterminate" aria-hidden="true">
              <div />
            </div>
          )}
        </section>
      )}

      <section className="log">
        <h2>Activity</h2>
        {job.log.length === 0 ? (
          <p className="empty">
            {keepAwake
              ? "Keeps your computer from going to sleep using the system's power settings. It generates no input and needs no special permissions."
              : "Nothing yet. Press Start and the first action happens after one interval."}
          </p>
        ) : (
          <ol>
            {job.log.map((l) => (
              <li key={l.id} className={l.error ? "error" : undefined}>
                <time>{l.time.toLocaleTimeString()}</time>
                <span>{l.text}</span>
              </li>
            ))}
          </ol>
        )}
      </section>
    </main>
  );
}
