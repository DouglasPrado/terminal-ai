import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { InvisibleModeBadge } from "./InvisibleModeBadge";

describe("invisible mode indicator", () => {
  afterEach(cleanup);

  it("is visible while the mode is on", () => {
    render(<InvisibleModeBadge active />);
    expect(screen.getByTestId("invisible-mode-indicator").textContent).toMatch(/Invisível/);
  });

  it("is absent while the mode is off", () => {
    render(<InvisibleModeBadge active={false} />);
    expect(screen.queryByTestId("invisible-mode-indicator")).toBeNull();
  });
});
