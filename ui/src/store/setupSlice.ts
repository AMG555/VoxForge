import { createSlice, PayloadAction } from '@reduxjs/toolkit';

export interface SetupState {
  isDesktop: boolean;
  desktopPlatform: string;
  desktopVersion: string;
  serverBaseUrl: string;
  isWizardOpen: boolean;
  selectedProfile: 'rust_core' | 'core_neural' | 'full_cloning';
  runtimeDirDraft: string;
  customPythonDraft: string;
  optionsExpanded: boolean;
  detailsExpanded: boolean;
  remoteExpanded: boolean;
  remoteUrlDraft: string;
  telemetryOptIn: boolean;
}

const initialState: SetupState = {
  isDesktop: false,
  desktopPlatform: 'web',
  desktopVersion: typeof __APP_VERSION__ !== 'undefined' ? __APP_VERSION__ : '0.7.1',
  serverBaseUrl: '',
  isWizardOpen: false,
  selectedProfile: 'rust_core',
  runtimeDirDraft: '',
  customPythonDraft: '',
  optionsExpanded: false,
  detailsExpanded: false,
  remoteExpanded: false,
  remoteUrlDraft:
    typeof window !== 'undefined' ? localStorage.getItem('voxforge_remote_url') || '' : '',
  telemetryOptIn: false,
};

const setupSlice = createSlice({
  name: 'setup',
  initialState,
  reducers: {
    setDesktopMeta(
      state,
      action: PayloadAction<{
        isDesktop: boolean;
        platform: string;
        version: string;
        serverBaseUrl: string;
      }>
    ) {
      state.isDesktop = action.payload.isDesktop;
      state.desktopPlatform = action.payload.platform;
      state.desktopVersion = action.payload.version;
      state.serverBaseUrl = action.payload.serverBaseUrl;
    },
    setWizardOpen(state, action: PayloadAction<boolean>) {
      state.isWizardOpen = action.payload;
    },
    setSelectedProfile(
      state,
      action: PayloadAction<'rust_core' | 'core_neural' | 'full_cloning'>
    ) {
      state.selectedProfile = action.payload;
    },
    setRuntimeDirDraft(state, action: PayloadAction<string>) {
      state.runtimeDirDraft = action.payload;
    },
    setCustomPythonDraft(state, action: PayloadAction<string>) {
      state.customPythonDraft = action.payload;
    },
    setOptionsExpanded(state, action: PayloadAction<boolean>) {
      state.optionsExpanded = action.payload;
    },
    setDetailsExpanded(state, action: PayloadAction<boolean>) {
      state.detailsExpanded = action.payload;
    },
    setRemoteExpanded(state, action: PayloadAction<boolean>) {
      state.remoteExpanded = action.payload;
    },
    setRemoteUrlDraft(state, action: PayloadAction<string>) {
      state.remoteUrlDraft = action.payload;
    },
    setTelemetryOptIn(state, action: PayloadAction<boolean>) {
      state.telemetryOptIn = action.payload;
    },
  },
});

export const {
  setDesktopMeta,
  setWizardOpen,
  setSelectedProfile,
  setRuntimeDirDraft,
  setCustomPythonDraft,
  setOptionsExpanded,
  setDetailsExpanded,
  setRemoteExpanded,
  setRemoteUrlDraft,
  setTelemetryOptIn,
} = setupSlice.actions;

export default setupSlice.reducer;
