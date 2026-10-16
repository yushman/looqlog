import { describe, expect, it } from "vitest";

import { newSincePause, resumeLabel } from "./resume-label";

describe("resumeLabel", () => {
  it("shows the new-line count with thousands separators and none at zero", () => {
    expect(resumeLabel(0)).toBe("Resume following");
    expect(resumeLabel(142)).toBe("Resume following · 142 new");
    expect(resumeLabel(12408)).toBe("Resume following · 12,408 new");
  });
});

describe("newSincePause", () => {
  it("is the difference, clamped at zero", () => {
    expect(newSincePause(150, 100)).toBe(50);
    expect(newSincePause(90, 100)).toBe(0);
  });
});
