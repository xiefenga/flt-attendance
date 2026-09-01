# FLT Attendance

固定读取钉钉考勤导出并生成固定格式考勤统计表的离线桌面应用。

应用当前执行口径见 [当前考勤规则](docs/当前考勤规则.md)，完整实现边界和 AI 开发约束见 [考勤规则与实现规范](docs/考勤规则与实现规范.md)，尚未明确的口径统一记录在 [考勤统计待确认事项](docs/待确认事项.md)。

## 技术结构

- `attendance-core`：钉钉工作簿检查与统计表生成。
- `attendance-cli`：本地验证命令行入口。
- `attendance-napi`：供 Electron 调用的 N-API 原生模块。
- `src/main` / `src/preload`：安全的 Electron 桌面桥接层。
- `src/renderer`：React 操作界面。

## 启动桌面应用

需要 Node.js 24、npm 11 和 Rust 1.95。首次启动先安装依赖：

```bash
npm install
npm run dev
```

应用启动后选择钉钉完整考勤报表，统计年月从“每日统计”表的日期列读取。工作簿可包含“入职名单”和“离职名单”，程序会据此按在职区间计算，并处理入职当天餐补例外。

首页提供“下载输入模板”按钮，可将符合程序读取结构的钉钉考勤输入模板保存到指定位置。模板包含必需的打卡时间、原始记录、月度汇总和每日统计 Sheet，以及可选的入职名单、离职名单和年假明细 Sheet。

主界面的“设置”包含：

- 特殊人员：配置餐补例外、全部或仅工作日/周末不计算加班时长的人员、六天工作制人员，以及 08:30 到岗分界的弹性下班人员。
- 不参与考勤人员：配置后从考勤明细、汇总表和异常明细中完全排除。

设置保存在系统应用数据目录的 `attendance-settings.json`，支持导入和导出。

## 构建

```bash
npm run build
npm run package
```

`package` 在当前系统生成可运行的应用目录。Windows 安装包需要在 Windows x64 环境执行：

```powershell
npm install
npm run make
```

仓库包含 [Windows 构建工作流](.github/workflows/windows-build.yml)，推送、拉取请求或手动触发后会运行 Rust 测试、构建 Squirrel.Windows 安装包并上传构建产物。

## 示例与规则资料

- `examples/input/`：钉钉完整考勤报表示例。
- `examples/templates/`：当前输出模板和旧版模板。
- `docs/references/`：原始规则 Excel 和补充 DOCX。

示例考勤报表含真实历史人员及考勤信息。仓库公开前必须先替换为脱敏数据。

## Rust 命令行验证

```bash
cargo run -p attendance-cli -- inspect <钉钉导出.xlsx>
cargo run -p attendance-cli -- generate <输出.xlsx> 2026 7
cargo run -p attendance-cli -- calculate <钉钉导出.xlsx> <输出.xlsx>
```
