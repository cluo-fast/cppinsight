import React, { useEffect, useState } from "react";
import { previewSource } from "./api";
import type { SourcePreview } from "./types";

export default function SourcePreviewPanel({ root, projectId, file, line, generation, onClose }: {
  root: string; projectId: number; file: string; line: number; generation: number; onClose: () => void;
}) {
  const [result, setResult] = useState<SourcePreview | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let cancelled = false;
    setResult(null); setError("");
    void previewSource(root, projectId, file, line)
      .then(data => { if (!cancelled) setResult(data); })
      .catch(error => { if (!cancelled) setError(String(error)); });
    return () => { cancelled = true; };
  }, [root, projectId, file, line, generation]);
  return <section className="source-preview" aria-label="源码证据预览">
    <div className="graph-toolbar"><h3>源码证据 · {file}:{line}</h3><button className="secondary" onClick={onClose}>关闭源码预览</button></div>
    {error ? <p role="alert">{error}</p> : !result ? <p role="status">正在读取本地索引片段…</p> : <>
      <p className="muted">索引快照：{result.indexed_at}。此处展示索引时的源码；文件修改后请重新索引。</p>
      {result.truncated ? <p role="status">片段过长，部分内容已截断。</p> : null}
      <pre>{result.text.split("\n").map((text, index) => <div className={result.start_line + index === result.focus_line ? "source-focus" : ""} key={index}>
        <span className="source-line-number">{result.start_line + index}</span><code>{text}</code>
      </div>)}</pre>
    </>}
  </section>;
}
