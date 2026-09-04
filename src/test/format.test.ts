import { describe, expect, it } from "vitest";
import { formatBytes, formatDate, pageRange } from "../format";
import { errorMessage } from "../api";

describe("display helpers", () => {
  it("formats byte sizes without assuming a high-memory machine", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(2048)).toBe("2 KB");
    expect(formatBytes(16 * 1024 ** 3)).toBe("16.0 GB");
    expect(formatBytes(null)).toBe("Not available");
    expect(formatBytes(-1)).toBe("Not available");
  });
  it("handles empty and partial pages", () => {
    expect(pageRange(0, 60, 0)).toBe("0 photos");
    expect(pageRange(60, 60, 71)).toBe("61–71 of 71");
    expect(pageRange(0, 60, 3000)).toBe("1–60 of 3,000");
  });
  it("does not invent a timestamp", () => {
    expect(formatDate(null)).toBe("Not available");
    expect(formatDate("camera local time")).toBe("camera local time");
  });
  it("shows structured errors without dumping arbitrary IPC payloads", () => {
    expect(errorMessage({ code: "busy", message: "A scan is active" })).toBe(
      "A scan is active",
    );
    expect(errorMessage({ secret: "never display" })).not.toContain(
      "never display",
    );
  });
});
