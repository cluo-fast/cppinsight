function escapeHtml(value: string): string {
  return value.replace(/[&<>\"]/g, (character) => ({
    "&": "&amp;",
    "<": "&lt;",
    ">": "&gt;",
    "\"": "&quot;"
  }[character] ?? character));
}

/** Convert the deterministic Markdown client brief into a safe standalone HTML report. */
export function summaryAsHtml(markdown: string, title: string, graphSvg = "", graphView = ""): string {
  const body = markdown.split(/\r?\n/).map((line) => {
    if (line.startsWith("### ")) return `<h3>${escapeHtml(line.slice(4))}</h3>`;
    if (line.startsWith("## ")) return `<h2>${escapeHtml(line.slice(3))}</h2>`;
    if (line.startsWith("# ")) return `<h1>${escapeHtml(line.slice(2))}</h1>`;
    if (line.startsWith("- ")) return `<li>${escapeHtml(line.slice(2))}</li>`;
    return line.trim() ? `<p>${escapeHtml(line)}</p>` : "";
  }).join("\n").replace(/(<li>.*<\/li>\n?)+/g, (items) => `<ul>${items}</ul>`);
  const graphSection = graphSvg
    ? `<section><h2>关系图快照</h2>${graphView ? `<p><strong>视图：</strong>${escapeHtml(graphView)}</p>` : ""}${graphSvg}</section>`
    : "";
  return `<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><title>${escapeHtml(title)}</title><style>body{font:16px system-ui;max-width:960px;margin:40px auto;line-height:1.6;color:#1f2937}code{background:#f3f4f6;padding:2px 4px}h1,h2,h3{margin-top:1.5em}section svg{max-width:100%;height:auto;border:1px solid #d1d5db}</style></head><body>${body}${graphSection}</body></html>`;
}
