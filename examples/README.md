# 示例数据与模板

## 目录

- `input/`：按钉钉导出结构从零构造的合成考勤工作簿，用于集成测试和人工验收，场景见 [合成样例说明](input/README.md)。
- `templates/钉钉考勤输入模板.xlsx`：首页“下载输入模板”提供的脱敏钉钉报表，保留少量跨 Sheet 可关联的示例数据。
- `templates/考勤统计表模板.xlsx`：当前应用读取的历史考勤表模板，同时用于补全所属公司信息。

## 数据安全

`input/` 中的工作簿只包含 `SYN001` 至 `SYN012` 的虚构身份，出勤、请假、加班及入离职日期均为测试场景设计，不是对真实记录仅做姓名替换。工作簿为新建文件，不复制旧文件的隐藏内容、批注或元数据。

本次替换不清除 Git 历史，也不代表 `templates/`、`settings/` 或 `docs/references/` 已完成隐私审查。首页下载的输入模板必须继续保持脱敏；公开仓库前仍需单独检查其他文件和历史版本。

## 维护要求

1. 输入样例必须保留钉钉导出的四张工作表：`打卡时间`、`原始记录`、`月度汇总`、`每日统计`。
2. 更换模板时同步检查 `forge.config.cjs`、`src/main/index.ts` 和 Rust 集成测试中的路径。
3. 不要把应用生成的临时结果放入本目录；临时结果放在被 Git 忽略的 `outputs/`。
4. 修改合成样例后执行 `python scripts/verify-synthetic-input.py` 和 `cargo test --workspace --locked`，同步更新 `crates/attendance-core/tests/synthetic_export.rs`，不得重新引入真实员工资料。

