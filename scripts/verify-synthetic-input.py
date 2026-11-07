"""Audit the synthetic XLSX fixture using only the Python standard library."""

from collections import Counter
from pathlib import Path
import re
import sys
from xml.etree import ElementTree as ET
from zipfile import ZipFile


ROOT = Path(__file__).resolve().parents[1]
FIXTURE = ROOT / "examples/input/钉钉考勤报表示例_202607.xlsx"
NS = {"x": "http://schemas.openxmlformats.org/spreadsheetml/2006/main"}
SHEETS = ["打卡时间", "原始记录", "月度汇总", "每日统计", "入职名单", "离职名单", "年假明细"]
IDENTITIES = {f"SYN{i:03}": f"合成员工{i:02}" for i in range(1, 13)}
LABELS = {
    "合成考勤样例（全部为虚构测试数据）", "统计周期：2026-07-01 至 2026-07-31",
    "姓名", "考勤组", "部门", "工号", "职位", "UserId", "日期", "星期", "班次",
    "打卡类型", "打卡时间", "打卡结果", "打卡地点", "考勤日期", "出勤天数",
    "工作日加班", "休息日加班", "节假日加班", "加班总时长", "事假", "调休",
    "病假", "年假", "产假", "陪产假", "婚假", "例假", "丧假", "哺乳假",
    "迟到次数", "严重迟到次数", "旷工迟到天数", "早退次数", "上班缺卡次数",
    "下班缺卡次数", "旷工天数", "公司", "入职日期", "离职日期",
    "截止当前月剩余（小时）", "合成考勤组", "合成测试部", "人事行政部",
    "合成示例公司", "测试岗位", "合成测试地点", "上班", "下班", "正常", "休息",
    "迟到", "缺卡", "当前不在可打卡的时间范围", "默认班次 08:30-17:30",
    "四小时班次 08:00-12:00",
    "正常,加班07-01 17:30到07-01 20:30 3小时",
    "休息,加班07-04 08:30到07-04 17:30 8小时",
    "年假07-06 08:30到07-06 17:30 8小时",
    "育儿假07-07 08:30到07-10 17:30 32小时",
    "事假07-13 08:30到07-13 17:30 8小时",
    "病假07-14 08:30到07-14 17:30 8小时",
    "调休07-15 08:30到07-15 17:30 8小时",
    "出差07-23 08:30到07-24 17:30 2天",
    "居家办公07-16 08:30到07-16 17:30 1天",
}
LABELS.update(IDENTITIES)
LABELS.update(IDENTITIES.values())
LABELS.update(f"synthetic-user-{i:03}" for i in range(1, 13))
LABELS.update(f"{i}日" for i in range(1, 32))
LABELS.update(f"预留字段{i}" for i in range(1, 42))
LABELS.update(f"预留{i}" for i in range(1, 7))
LABELS.update(f"星期{i}" for i in "日一二三四五六")
LABELS.update(f"{kind}{i}打卡{field}" for kind in ["上班", "下班"] for i in range(1, 4) for field in ["时间", "结果"])
DATE_OR_TIME = re.compile(r"(?:26-07-(?:0[1-9]|[12]\d|3[01])|2026-07-(?:0[1-9]|[12]\d|3[01])(?: (?:[01]\d|2[0-3]):[0-5]\d)?|(?:[01]\d|2[0-3]):[0-5]\d)")


def require(condition, message):
    if not condition:
        raise ValueError(message)


def audit(path):
    with ZipFile(path) as archive:
        allowed_parts = {
            "xl/workbook.xml", "xl/styles.xml", "xl/theme/theme1.xml",
            "xl/sharedStrings.xml", "_rels/.rels", "xl/_rels/workbook.xml.rels",
            "[Content_Types].xml",
            *(f"xl/worksheets/sheet{i}.xml" for i in range(1, 8)),
        }
        require(set(archive.namelist()) == allowed_parts, "Unexpected package part (metadata, comments, links, or embedded data).")
        require(len(archive.namelist()) == len(allowed_parts), "Duplicate ZIP part.")
        workbook = ET.fromstring(archive.read("xl/workbook.xml"))
        sheets = workbook.findall("x:sheets/x:sheet", NS)
        require([sheet.get("name") for sheet in sheets] == SHEETS, "Unexpected worksheet list.")
        require(all(sheet.get("state", "visible") == "visible" for sheet in sheets), "Hidden worksheet.")
        require(workbook.find("x:definedNames", NS) is None, "Unexpected defined names.")
        for part in ["_rels/.rels", "xl/_rels/workbook.xml.rels"]:
            relationships = ET.fromstring(archive.read(part))
            require(all(r.get("TargetMode") != "External" for r in relationships), "External relationship.")

        strings = ["".join(si.itertext()) for si in ET.fromstring(archive.read("xl/sharedStrings.xml"))]
        for index, value in enumerate(strings):
            # Do not print unexpected content: a failed check may be caused by real data.
            require(not value or value in LABELS or DATE_OR_TIME.fullmatch(value), f"Unapproved shared string at index {index}.")

        parsed = {}
        for index, name in enumerate(SHEETS, 1):
            sheet = ET.fromstring(archive.read(f"xl/worksheets/sheet{index}.xml"))
            require(not any(node.get("hidden") in ("1", "true") for node in sheet.iter()), f"Hidden row/column in {name}.")
            require(sheet.find(".//x:f", NS) is None, f"Unexpected formula in {name}.")
            require(sheet.find(".//x:hyperlink", NS) is None, f"Unexpected hyperlink in {name}.")
            rows = []
            for row in sheet.findall("x:sheetData/x:row", NS):
                values = {}
                for cell in row.findall("x:c", NS):
                    value = cell.find("x:v", NS)
                    if value is None or value.text is None:
                        require(cell.find("x:is", NS) is None, f"Unexpected inline string in {name}.")
                        continue
                    kind = cell.get("t", "n")
                    require(kind in ("s", "str", "n"), f"Unexpected cell type in {name}.")
                    text = strings[int(value.text)] if kind == "s" else value.text
                    if kind in ("s", "str"):
                        require(not text or text in LABELS or DATE_OR_TIME.fullmatch(text), f"Unapproved text in {name} at {cell.get('r')}.")
                    else:
                        require(re.fullmatch(r"-?\d+(?:\.\d+)?", text), f"Invalid numeric cell in {name}.")
                    values[re.sub(r"\d", "", cell.get("r"))] = text
                rows.append((int(row.get("r")), values))
            parsed[name] = rows

        starts = {"打卡时间": 5, "原始记录": 4, "月度汇总": 5, "每日统计": 5, "入职名单": 2, "离职名单": 2, "年假明细": 4}
        counts = {}
        for name, rows in parsed.items():
            data = [values for row, values in rows if row >= starts[name] and values]
            counts[name] = len(data)
            for values in data:
                number, person = (values.get("A"), values.get("B")) if name == "年假明细" else (values.get("C" if name in ["入职名单", "离职名单"] else "D"), values.get("A"))
                require(number in IDENTITIES and IDENTITIES[number] == person, f"Non-synthetic or mismatched identity in {name}.")
                if name in SHEETS[:4]:
                    require(values.get("F") == f"synthetic-user-{int(number[3:]):03}", f"Mismatched UserId in {name}.")
            if name == "每日统计":
                pairs = Counter((v["D"], v["G"]) for v in data)
                expected = Counter((number, f"26-07-{day:02}") for number in IDENTITIES for day in range(1, 32))
                require(pairs == expected, "Missing/duplicate daily identities or dates.")
            if name in ["月度汇总", "年假明细"]:
                column = "D" if name == "月度汇总" else "A"
                require(Counter(v[column] for v in data) == Counter(IDENTITIES.keys()), f"Missing/duplicate employee in {name}.")
        require(counts == dict(zip(SHEETS, [451, 452, 12, 372, 1, 1, 12])), "Unexpected fixture row counts.")
        print("Synthetic input audit passed: 7 visible sheets, 12 synthetic employees, 372 daily records; no extra package parts or unapproved text.")


if __name__ == "__main__":
    try:
        audit(Path(sys.argv[1]) if len(sys.argv) > 1 else FIXTURE)
    except (ValueError, KeyError, ET.ParseError) as error:
        raise SystemExit(f"Synthetic input audit failed: {error}") from error
