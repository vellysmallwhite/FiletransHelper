import type { AgentStatusEvent, LocalInfo } from "../api/types";

export type SettingsState = {
  localInfo: LocalInfo | null;
  lastEvent: AgentStatusEvent | null;
  error: string | null;
};

export const initialSettingsState: SettingsState = {
  localInfo: null,
  lastEvent: null,
  error: null,
};
