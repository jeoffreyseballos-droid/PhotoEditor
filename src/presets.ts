export type BuiltInPresetId = "pop" | "warm" | "black_and_white";

export interface BuiltInPreset {
  id: BuiltInPresetId;
  name: string;
  description: string;
  version: string;
}

export interface PresetEditingState {
  selected_asset_ids: string[];
  applied_preset: BuiltInPresetId | null;
  applied_count: number;
  unresolved_subject_masks: string[];
}

export interface PresetApplyResult {
  preset: BuiltInPreset;
  selected_asset_ids: string[];
  recipes_updated: number;
  recipes_unchanged: number;
  unresolved_subject_masks: string[];
}
