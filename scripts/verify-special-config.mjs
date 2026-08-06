import fs from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);
const api = require("../resources/native/attendance.node");
const [inputPath, outputPath, historyPath, configPath] = process.argv.slice(2);
const config = fs.readFileSync(configPath, "utf8");
const report = JSON.parse(api.generateReport(inputPath, outputPath, historyPath, config));
const configured = JSON.parse(config);
const employeeNumbers = new Set(
  [
    ...configured.specialPersonnel.punchMealNoOvertime,
    ...configured.specialPersonnel.noPunchMealNoOvertime,
    ...configured.specialPersonnel.noMealNoOvertime
  ]
    .map((person) => person.employeeNo)
    .filter(Boolean)
);
const matched = report.detail_rows
  .filter((row) => employeeNumbers.has(row.employee_no))
  .map((row) => ({
    employeeNo: row.employee_no,
    name: row.name,
    overtime:
      row.summary.weekday_overtime_hours
      + row.summary.weekend_overtime_hours
      + row.summary.holiday_overtime_hours,
    dailyOvertime: row.days.reduce((sum, day) => sum + day.overtime_hours, 0)
  }));

if (matched.some((row) => row.overtime !== 0 || row.dailyOvertime !== 0)) {
  throw new Error("特殊人员仍包含加班时长");
}
const excludedNumbers = new Set(
  configured.excludedPersonnel.map((person) => person.employeeNo).filter(Boolean)
);
const leaked = report.detail_rows.filter((row) => excludedNumbers.has(row.employee_no));
if (leaked.length) throw new Error("不参与考勤人员仍出现在结果中");
console.log(JSON.stringify({
  rows: report.detail_rows.length,
  matchedSpecialPersonnel: matched.length,
  excludedPersonnel: excludedNumbers.size,
  warnings: report.warnings
}, null, 2));
