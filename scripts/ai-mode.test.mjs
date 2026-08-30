import assert from "node:assert/strict";
import test from "node:test";

const aiMode = await import("../desktop/src/aiMode.ts");

test("AI mode label makes local-only mode explicit", () => {
  assert.equal(aiMode.aiModeLabel(false), "AI 已关闭，仅本地分析");
  assert.equal(aiMode.aiModeLabel(true), "AI 请求已启用");
});

test("local-only mode can be saved without an API key", () => {
  assert.equal(aiMode.canSaveAiSettings(false, false, ""), true);
  assert.equal(aiMode.canSaveAiSettings(true, false, ""), false);
  assert.equal(aiMode.canSaveAiSettings(true, true, ""), true);
  assert.equal(aiMode.canSaveAiSettings(true, false, " sk-test "), true);
});

test("context confirmation explains whether model transmission is enabled", () => {
  assert.equal(
    aiMode.askContextDescription(false, 3),
    "AI 已关闭；本次只返回本地检索结果，不会向模型发送内容。"
  );
  assert.equal(
    aiMode.askContextDescription(true, 3),
    "本次回答将使用 3 个本地片段；只发送下列文件和行号范围，绝不自动上传整个项目。"
  );
});

test("local-only preview copy never promises a model send", () => {
  assert.deepEqual(aiMode.askPreviewCopy(false), {
    ariaLabel: "本地检索结果预览",
    title: "本地检索结果",
    status: "正在整理本地检索结果…",
    confirm: "确认本地检索",
    empty: "没有找到匹配源码片段；仅返回本地检索结果。"
  });
  assert.deepEqual(aiMode.askPreviewCopy(true), {
    ariaLabel: "发送前上下文预览",
    title: "发送前确认上下文",
    status: "正在整理将发送的代码片段…",
    confirm: "确认发送",
    empty: "没有找到匹配源码片段；不会向模型发送源码上下文。"
  });
});
