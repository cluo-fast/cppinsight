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

export function askPreviewCopy(aiEnabled: boolean): {
  ariaLabel: string;
  title: string;
  status: string;
  confirm: string;
  empty: string;
} {
  if (!aiEnabled) {
    return {
      ariaLabel: "本地检索结果预览",
      title: "本地检索结果",
      status: "正在整理本地检索结果…",
      confirm: "确认本地检索",
      empty: "没有找到匹配源码片段；仅返回本地检索结果。"
    };
  }
  return {
    ariaLabel: "发送前上下文预览",
    title: "发送前确认上下文",
    status: "正在整理将发送的代码片段…",
    confirm: "确认发送",
    empty: "没有找到匹配源码片段；不会向模型发送源码上下文。"
  };
}
