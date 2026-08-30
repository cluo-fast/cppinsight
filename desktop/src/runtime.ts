type TauriWindow = Window & {
  __TAURI_INTERNALS__?: unknown;
};

export const BROWSER_PREVIEW_MESSAGE = "浏览器预览模式：请在 CppInsight AI 桌面应用中打开项目。";

export function isTauriRuntime(): boolean {
  if (typeof window === "undefined") {
    return false;
  }
  return Boolean((window as TauriWindow).__TAURI_INTERNALS__);
}

export function desktopActionMessage(): string | null {
  return isTauriRuntime() ? null : BROWSER_PREVIEW_MESSAGE;
}

export function isDesktopActionDisabled(): boolean {
  return desktopActionMessage() !== null;
}
