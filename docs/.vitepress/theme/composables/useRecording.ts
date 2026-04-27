export interface RecordingEvent {
  t: number;
  type: "frame" | "slider" | "nav";
  frame?: number;
  index?: number;
  value?: number;
  event?: string;
}

export interface Recording {
  events: RecordingEvent[];
}

export interface ProcessedRecording {
  duration: number;
  /** Per-slider sorted [{t, value}] arrays (index 0 and 1). */
  sliderEvents: Array<Array<{ t: number; value: number }>>;
  /** Pre-accumulated encoder rotation angle per nav event. */
  encRotEvents: Array<{ t: number; cumAngle: number }>;
  /** Timestamps of encoder Select events (for press-dip animation). */
  encSelectTimes: Array<{ t: number }>;
  /** GIF frame index events. */
  frameEvents: Array<{ t: number; frame: number }>;
}

const DEG_PER_CLICK = 15;
const PADDING = 0.5; // seconds of rest pose before/after recording

/**
 * Parse raw recording JSON into pre-processed animation tracks offset by PADDING.
 * All returned time values already include the PADDING offset.
 */
export function useRecording(recording: Recording): ProcessedRecording {
  const events = recording.events;
  const lastT = events.length > 0 ? Math.max(...events.map((e) => e.t)) : 0;
  const duration = lastT + 2 * PADDING;

  const sliderEvents: Array<Array<{ t: number; value: number }>> = [[], []];
  const encRotEvents: Array<{ t: number; cumAngle: number }> = [];
  const encSelectTimes: Array<{ t: number }> = [];
  const frameEvents: Array<{ t: number; frame: number }> = [];

  let cumAngleDeg = 0;
  for (const e of events) {
    const t = e.t + PADDING;
    if (e.type === "slider" && e.index !== undefined && e.value !== undefined) {
      sliderEvents[e.index]?.push({ t, value: e.value });
    } else if (e.type === "nav") {
      if (e.event === "Up") {
        cumAngleDeg += DEG_PER_CLICK;
        encRotEvents.push({ t, cumAngle: cumAngleDeg });
      } else if (e.event === "Down") {
        cumAngleDeg -= DEG_PER_CLICK;
        encRotEvents.push({ t, cumAngle: cumAngleDeg });
      } else if (e.event === "Select") {
        encSelectTimes.push({ t });
      }
    } else if (e.type === "frame" && e.frame !== undefined) {
      frameEvents.push({ t, frame: e.frame });
    }
  }

  return { duration, sliderEvents, encRotEvents, encSelectTimes, frameEvents };
}

/** Binary search: index of last entry with entry.t <= t, or -1 if none. */
export function bsearch<T extends { t: number }>(arr: T[], t: number): number {
  let lo = 0, hi = arr.length - 1, fi = -1;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (arr[mid].t <= t) { fi = mid; lo = mid + 1; }
    else hi = mid - 1;
  }
  return fi;
}
