export function aiModeLabel(enabled: boolean): string {
  return enabled ? "AI 请求已启用" : "AI 已关闭，仅本地分析";
}

export function canSaveAiSettings(
  aiEnabled: boolean,
  apiKeyConfigured: boolean,
  pendingApiKey: string
): boolean {
  return !aiEnabled || apiKeyConfigured || pendingApiKey.trim().length > 0;
}

export function askContextDescription(aiEnabled: boolean, citationCount: number): string {
  if (!aiEnabled) {
    return "AI 已关闭；本次只返回本地检索结果，不会向模型发送内容。";
  }
  return `本次回答将使用 ${citationCount} 个本地片段；只发送下列文件和行号范围，绝不自动上传整个项目。`;
}
