import fs from "node:fs/promises";
import path from "node:path";

import { FileBlob, SpreadsheetFile } from "@oai/artifact-tool";

const sourcePath = path.resolve(process.argv[2]);
const renderDirectory = path.resolve(process.argv[3] ?? "outputs/workbook-renders");
const workbook = await SpreadsheetFile.importXlsx(await FileBlob.load(sourcePath));

const summary = await workbook.inspect({
  kind: "workbook,sheet",
  maxChars: 12_000,
  tableMaxRows: 8,
  tableMaxCols: 12
});
console.log(summary.ndjson);

for (const request of [
  { kind: "region", sheetId: "考勤明细", range: "A1:BB12", maxChars: 18_000 },
  { kind: "region", sheetId: "考勤汇总表", range: "A1:T12", maxChars: 12_000 },
  { kind: "region", sheetId: "异常打卡明细", range: "A1:K12", maxChars: 12_000 },
  {
    kind: "match",
    searchTerm: "#REF!|#DIV/0!|#VALUE!|#NAME\\?|#N/A",
    options: { useRegex: true, maxResults: 100 },
    summary: "formula error scan"
  }
]) {
  const result = await workbook.inspect(request);
  console.log(result.ndjson);
}

await fs.mkdir(renderDirectory, { recursive: true });
for (const [sheetName, range] of [
  ["考勤明细", "A1:BB14"],
  ["考勤明细", "A476:BB481"],
  ["考勤汇总表", "A1:T14"],
  ["异常打卡明细", "A1:K14"]
]) {
  const suffix = range.replaceAll(":", "-");
  const preview = await workbook.render({ sheetName, range, scale: 1.6, format: "png" });
  const outputPath = path.join(renderDirectory, `${sheetName}-${suffix}.png`);
  await fs.writeFile(outputPath, new Uint8Array(await preview.arrayBuffer()));
  console.log(`RENDERED:${outputPath}`);
}
