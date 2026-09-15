import { defineConfig } from "vitest/config";

export default defineConfig({
  test: { include: ["test/**/*.test.jsx"], environment: "node", pool: "forks", maxWorkers: 1, minWorkers: 1 }
});
