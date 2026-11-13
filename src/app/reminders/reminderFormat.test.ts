import { describe, expect, it } from "vitest";
import type { Reminder } from "@/bindings";
import {
  dayLabel,
  dueInstant,
  groupReminders,
  isMissed,
  isoDay,
  linkInfo,
  linkLabel,
  monthGrid,
  remindersByDay,
  shiftMonth,
} from "./reminderFormat";

function reminder(
  date: string,
  time: string | null,
  status: Reminder["status"] = "pending",
): Reminder {
  return {
    id: `${date}-${time}-${status}`,
    title: "t",
    date,
    time,
    links: [],
    details: null,
    kind: "notify",
    status,
    ring_at_ms: null,
    source: "user",
    created_ms: 0,
    updated_ms: 0,
  };
}

describe("reminder formatting", () => {
  const now = new Date(2026, 10, 13, 15, 0); // Fri 13 Nov 2026, 3 PM

  it("labels nearby days by name", () => {
    expect(isoDay(now)).toBe("2026-11-13");
    expect(dayLabel("2026-11-13", "2026-11-13", "en-GB")).toBe("Today");
    expect(dayLabel("2026-11-14", "2026-11-13", "en-GB")).toBe("Tomorrow");
    expect(dayLabel("2026-11-12", "2026-11-13", "en-GB")).toBe("Yesterday");
    expect(dayLabel("2026-12-31", "2026-12-30", "en-GB")).toBe("Tomorrow");
    expect(dayLabel("2026-11-25", "2026-11-13", "en-GB")).toContain("25");
    expect(dayLabel("2027-01-05", "2026-11-13", "en-GB")).toContain("2027");
  });

  it("groups ringing, earlier, per day and done in order", () => {
    const groups = groupReminders(
      [
        reminder("2026-11-25", "18:00"),
        reminder("2026-11-13", "17:00"),
        reminder("2026-11-13", null),
        reminder("2026-11-10", "09:00"),
        reminder("2026-11-13", "14:00", "ringing"),
        reminder("2026-11-01", null, "done"),
      ],
      now,
      "en-GB",
    );
    expect(groups.map((g) => g.tone)).toEqual([
      "ringing",
      "earlier",
      "today",
      "upcoming",
      "done",
    ]);
    // All-day sorts before timed within a day.
    expect(groups[2].items.map((r) => r.time)).toEqual([null, "17:00"]);
  });

  it("knows when a ringing reminder was missed", () => {
    expect(dueInstant(reminder("2026-11-13", null)).getHours()).toBe(9);
    expect(isMissed(reminder("2026-11-13", "14:59"), now)).toBe(false);
    expect(isMissed(reminder("2026-11-13", "09:00"), now)).toBe(true);
  });

  it("names meeting links and keeps page paths", () => {
    expect(linkInfo("https://meet.google.com/abc-defg-hij")).toEqual({
      name: "Google Meet",
      detail: "meet.google.com/abc-defg-hij",
      kind: "meeting",
    });
    expect(linkInfo("https://us02web.zoom.us/j/123").name).toBe("Zoom");
    expect(linkInfo("https://www.figma.com/file/x?y=1")).toEqual({
      name: "figma.com",
      detail: "/file/x?y=1",
      kind: "web",
    });
    expect(linkInfo("https://example.com/").detail).toBe("");
    expect(linkInfo("zoommtg://zoom.us/join?confno=1").kind).toBe("meeting");
    expect(linkInfo("spotify:track:1").kind).toBe("app");
  });

  it("lays out whole weeks for a month", () => {
    // November 2026 starts on a Sunday and ends on a Monday.
    const monday = monthGrid("2026-11-13", 1);
    expect(monday[0]).toBe("2026-10-26");
    expect(monday[monday.length - 1]).toBe("2026-12-06");
    expect(monday.length).toBe(42);
    const sunday = monthGrid("2026-11-13", 0);
    expect(sunday[0]).toBe("2026-11-01");
    expect(sunday[sunday.length - 1]).toBe("2026-12-05");
    expect(sunday.length).toBe(35);
    // February 2026 fits exactly four Sunday-first weeks.
    expect(monthGrid("2026-02-10", 0).length).toBe(28);
    expect(shiftMonth("2026-12-31", 1)).toBe("2027-01-01");
    expect(shiftMonth("2026-01-15", -1)).toBe("2025-12-01");
  });

  it("buckets reminders by day in due order", () => {
    const days = remindersByDay([
      reminder("2026-11-13", "17:00"),
      reminder("2026-11-14", null),
      reminder("2026-11-13", null),
    ]);
    expect([...days.keys()]).toEqual(["2026-11-13", "2026-11-14"]);
    expect(days.get("2026-11-13")?.map((r) => r.time)).toEqual([null, "17:00"]);
  });

  it("shows a short host for links", () => {
    expect(linkLabel("https://meet.google.com/abc-defg-hij")).toBe(
      "meet.google.com",
    );
    expect(linkLabel("https://www.zoom.us/j/1")).toBe("zoom.us");
    expect(linkLabel("zoommtg://zoom.us/join?confno=1")).toBe("zoommtg");
  });
});
