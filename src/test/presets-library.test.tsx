import { render, screen } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { api } from "../api";
import { PresetsScreen } from "../screens/PresetsScreen";

vi.mock("../api", () => ({
  api: {
    builtinPresets: vi.fn(),
    trainedStyles: vi.fn(),
  },
  errorMessage: () => "Preset loading failed",
}));

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.builtinPresets).mockResolvedValue([
    { id: "pop", name: "POP", description: "Bright", version: "1" },
  ]);
  vi.mocked(api.trainedStyles).mockImplementation(async (photoType) =>
    photoType === "portrait"
      ? [
          {
            style_id: "portrait-v1",
            name: "My Portrait Style v1",
            version: "1.0.0",
            model_version: "model-v1",
            package_identity: "a".repeat(64),
            photo_type: "portrait",
            description: "Locally trained",
            development_only: false,
          },
        ]
      : [],
  );
});

it("shows trained styles and built-ins in the top-level Presets section", async () => {
  render(<PresetsScreen onClose={vi.fn()} />);
  expect(await screen.findByText("My Portrait Style v1")).toBeInTheDocument();
  expect(screen.getByText("POP")).toBeInTheDocument();
  expect(api.trainedStyles).toHaveBeenCalledTimes(3);
});
