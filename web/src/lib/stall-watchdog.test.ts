import { describe, expect, it } from "vitest";
import { StallWatchdog, startGapTarget, type PlayheadSample } from "./stall-watchdog";

const playing = (t: number, ahead = 30): PlayheadSample => ({
  currentTime: t,
  paused: false,
  seeking: false,
  ended: false,
  bufferedAhead: ahead,
});

const ranges = (...r: [number, number][]): TimeRanges => ({
  length: r.length,
  start: (i: number) => r[i][0],
  end: (i: number) => r[i][1],
});

describe("StallWatchdog", () => {
  it("nudges, then recovers, when stuck with buffer ahead", () => {
    const dog = new StallWatchdog();
    let t = 0.267;
    const actions = Array.from({ length: 7 }, (_, i) => {
      const a = dog.tick(playing(t), i * 1000);
      if (a === "nudge") t += 0.1; // what the player does
      return a;
    });
    // First tick only records the position.
    expect(actions).toEqual(["none", "none", "none", "nudge", "none", "none", "recover"]);
  });

  it("stands down when the nudge gets playback going", () => {
    const dog = new StallWatchdog();
    let t = 1;
    for (let i = 0; i < 4; i++) if (dog.tick(playing(t), i * 1000) === "nudge") t += 0.1;
    for (let i = 4; i < 12; i++) expect(dog.tick(playing((t += 1)), i * 1000)).toBe("none");
  });

  it("leaves a moving, paused or starved player alone", () => {
    const moving = new StallWatchdog();
    for (let i = 0; i < 10; i++) expect(moving.tick(playing(i), i * 1000)).toBe("none");
    const paused = new StallWatchdog();
    for (let i = 0; i < 10; i++) expect(paused.tick({ ...playing(5), paused: true }, i * 1000)).toBe("none");
    const starved = new StallWatchdog();
    for (let i = 0; i < 10; i++) expect(starved.tick(playing(5, 1), i * 1000)).toBe("none");
  });

  it("caps rebuilds per minute", () => {
    const dog = new StallWatchdog();
    let t = 1;
    const recovers = Array.from({ length: 60 }, (_, i) => {
      const a = dog.tick(playing(t), i * 1000);
      if (a === "nudge") t += 0.1;
      return a;
    }).filter((a) => a === "recover");
    expect(recovers).toHaveLength(2);
  });
});

describe("startGapTarget", () => {
  it("jumps a small gap at the start", () => {
    expect(startGapTarget(ranges([0.2, 36.3]), 0)).toBe(0.2);
  });
  it("ignores no gap, big gaps, empty buffers and mid-file positions", () => {
    expect(startGapTarget(ranges([0, 36]), 0)).toBeNull();
    expect(startGapTarget(ranges([5, 36]), 0)).toBeNull();
    expect(startGapTarget(ranges(), 0)).toBeNull();
    expect(startGapTarget(ranges([600.2, 630]), 600)).toBeNull();
  });
});
