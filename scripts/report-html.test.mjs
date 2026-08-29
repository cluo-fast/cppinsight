import assert from "node:assert/strict";
import test from "node:test";
import { summaryAsHtml } from "../desktop/src/reportHtml.ts";

test("HTML summary escapes report content and preserves the trusted graph snapshot", () => {
  const html = summaryAsHtml(
    "# Project <alpha>\n\n- Evidence & notes",
    "Brief \"<draft>\"",
    '<svg aria-label="graph"><text>safe</text></svg>',
    "函数调用 <全部>"
  );

  assert.match(html, /<h1>Project &lt;alpha&gt;<\/h1>/);
  assert.match(html, /<li>Evidence &amp; notes<\/li>/);
  assert.match(html, /<title>Brief &quot;&lt;draft&gt;&quot;<\/title>/);
  assert.match(html, /<strong>视图：<\/strong>函数调用 &lt;全部&gt;/);
  assert.match(html, /<svg aria-label="graph"><text>safe<\/text><\/svg>/);
  assert.doesNotMatch(html, /Project <alpha>/);
});

test("HTML summary never turns a script payload into markup", () => {
  const html = summaryAsHtml("<script>alert('xss')</script>", "<unsafe>");
  assert.match(html, /&lt;script&gt;alert\('xss'\)&lt;\/script&gt;/);
  assert.doesNotMatch(html, /<script>/);
});
