import fs from "node:fs";
import path from "node:path";

const projectRoot = path.resolve(import.meta.dirname, "..");
const outputDirectory = path.join(
  projectRoot,
  "crates",
  "attendance-core",
  "data",
  "holidays"
);
const requestedYears = process.argv.slice(2).map(Number);
const years = requestedYears.length ? requestedYears : [new Date().getFullYear()];

function validateCalendar(data, expectedYear) {
  if (!data || data.year !== expectedYear) {
    throw new Error(`${expectedYear} 年节假日数据年份不匹配`);
  }
  if (!Array.isArray(data.papers) || !data.papers.some((url) =>
    typeof url === "string" && new URL(url).hostname.endsWith("gov.cn")
  )) {
    throw new Error(`${expectedYear} 年节假日数据缺少国务院公告来源`);
  }
  if (!Array.isArray(data.days) || data.days.length === 0) {
    throw new Error(`${expectedYear} 年节假日数据尚未发布`);
  }
  const dates = new Set();
  for (const day of data.days) {
    if (
      !day ||
      typeof day.name !== "string" ||
      !/^\d{4}-\d{2}-\d{2}$/.test(day.date) ||
      typeof day.isOffDay !== "boolean"
    ) {
      throw new Error(`${expectedYear} 年节假日数据格式错误`);
    }
    if (dates.has(day.date)) {
      throw new Error(`${expectedYear} 年节假日数据包含重复日期 ${day.date}`);
    }
    dates.add(day.date);
  }
}

fs.mkdirSync(outputDirectory, { recursive: true });
for (const year of years) {
  if (!Number.isInteger(year) || year < 2000 || year > 2100) {
    throw new Error(`无效节假日年份：${year}`);
  }
  const destination = path.join(outputDirectory, `${year}.json`);
  const source = `https://raw.githubusercontent.com/NateScarlet/holiday-cn/master/${year}.json`;
  try {
    const response = await fetch(source);
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const data = await response.json();
    validateCalendar(data, year);
    fs.writeFileSync(destination, `${JSON.stringify(data, null, 2)}\n`, "utf8");
    console.log(`Holiday calendar: ${destination}`);
  } catch (error) {
    if (!fs.existsSync(destination)) throw error;
    const cached = JSON.parse(fs.readFileSync(destination, "utf8"));
    validateCalendar(cached, year);
    console.warn(`${year} 年节假日联网更新失败，继续使用已校验的本地快照：${error}`);
  }
}
