import { setApiBaseUrl } from './api';

export interface DesktopContextInfo {
  is_desktop: boolean;
  server_base_url: string;
  data_dir: string;
  version: string;
  platform: string;
}

export function isTauriRuntime(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

export async function initDesktopApiTransport(): Promise<DesktopContextInfo | null> {
  const remoteOverride =
    typeof window !== 'undefined' ? localStorage.getItem('voxforge_remote_url') : null;

  if (!isTauriRuntime()) {
    if (remoteOverride && remoteOverride.trim()) {
      setApiBaseUrl(remoteOverride);
    }
    return null;
  }

  try {
    const { invoke } = await import('@tauri-apps/api/core');
    const info = await invoke<DesktopContextInfo>('get_desktop_context');
    if (remoteOverride && remoteOverride.trim()) {
      setApiBaseUrl(remoteOverride);
    } else if (info.server_base_url) {
      setApiBaseUrl(info.server_base_url);
    }
    return info;
  } catch (err) {
    console.warn('Tauri desktop context unavailable, using same-origin API:', err);
    return null;
  }
}

export async function pickRuntimeDirectoryDialog(initialPath?: string): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return await invoke<string | null>('pick_runtime_directory', { initialPath });
  } catch (err) {
    console.warn('Folder dialog failed:', err);
    return null;
  }
}

export async function pickPythonExecutableDialog(): Promise<string | null> {
  if (!isTauriRuntime()) return null;
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    return await invoke<string | null>('pick_python_executable');
  } catch (err) {
    console.warn('Python executable dialog failed:', err);
    return null;
  }
}

export async function openExternalUrl(url: string): Promise<boolean> {
  if (!isTauriRuntime()) {
    window.open(url, '_blank', 'noopener,noreferrer');
    return true;
  }
  try {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke('open_external_url', { url });
    return true;
  } catch (err) {
    console.warn('open_external_url failed:', err);
    return false;
  }
}

export async function minimizeDesktopWindow(): Promise<void> {
  if (!isTauriRuntime()) return;
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().minimize();
  } catch (err) {
    console.warn('minimize window failed:', err);
  }
}

export async function toggleMaximizeDesktopWindow(): Promise<void> {
  if (!isTauriRuntime()) return;
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().toggleMaximize();
  } catch (err) {
    console.warn('toggleMaximize window failed:', err);
  }
}

export async function closeDesktopWindow(): Promise<void> {
  if (!isTauriRuntime()) return;
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().close();
  } catch (err) {
    console.warn('close window failed:', err);
  }
}
