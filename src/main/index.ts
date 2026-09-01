import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { app, BrowserWindow, dialog, ipcMain, nativeTheme, shell } from "electron";
import squirrelStartup from "electron-squirrel-startup";

import type {
  AttendanceSettings,
  EmployeeIdentity,
  GenerateResponse,
  InspectResponse,
  SpecialPerson,
  SpecialPersonnelConfig,
  WorksheetSummary
} from "../shared/ipc-contract";

interface NativeReport {
  summary_rows: unknown[];
  exception_rows: unknown[];
  warnings: string[];
}

interface NativeApi {
  inspectDingtalk(path: string): string;
  generateReport(
    inputPath: string,
    outputPath: string,
    historyPath?: string,
    configJson?: string
  ): string;
}

const SPECIAL_GROUPS = [
  "punchMealNoOvertime",
  "weekdayWeekendPunchMealHolidayOvertime",
  "noPunchMealNoOvertime",
  "noMealNoOvertime",
  "flexibleArrivalShift",
  "sixDayNoMeal",
  "sixDayFourHourNoMeal"
] as const;

type SpecialGroup = (typeof SPECIAL_GROUPS)[number];

function personKey(person: SpecialPerson): string {
  return person.employeeNo ? `employee:${person.employeeNo}` : `name:${person.name}`;
}

function isAllowedSpecialOverlap(groups: ReadonlySet<SpecialGroup>): boolean {
  return groups.size === 2
    && groups.has("punchMealNoOvertime")
    && groups.has("flexibleArrivalShift");
}

function emptySettings(): AttendanceSettings {
  return {
    specialPersonnel: {
      punchMealNoOvertime: [],
      weekdayWeekendPunchMealHolidayOvertime: [],
      noPunchMealNoOvertime: [],
      noMealNoOvertime: [],
      flexibleArrivalShift: [],
      sixDayNoMeal: [],
      sixDayFourHourNoMeal: []
    },
    excludedPersonnel: [],
    statutoryHolidayDates: []
  };
}

const require = createRequire(__filename);
let mainWindow: BrowserWindow | null = null;
let nativeApi: NativeApi | null = null;

function nativeModulePath(): string {
  return app.isPackaged
    ? path.join(process.resourcesPath, "native", "attendance.node")
    : path.resolve(__dirname, "../../resources/native/attendance.node");
}

function historyTemplatePath(): string {
  return app.isPackaged
    ? path.join(process.resourcesPath, "考勤统计表模板.xlsx")
    : path.resolve(__dirname, "../../examples/templates/考勤统计表模板.xlsx");
}

function appIconPath(): string {
  return app.isPackaged
    ? path.join(process.resourcesPath, "icons", "app-icon.png")
    : path.resolve(__dirname, "../../resources/icons/app-icon.png");
}

function getNativeApi(): NativeApi {
  if (!nativeApi) nativeApi = require(nativeModulePath()) as NativeApi;
  return nativeApi;
}

function settingsPath(): string {
  return path.join(app.getPath("userData"), "attendance-settings.json");
}

function legacySpecialPersonnelPath(): string {
  return path.join(app.getPath("userData"), "special-personnel.json");
}

function normalizePerson(value: unknown): SpecialPerson {
  if (!value || typeof value !== "object") throw new Error("特殊人员配置格式错误");
  const record = value as Record<string, unknown>;
  const employeeNo = typeof record.employeeNo === "string" ? record.employeeNo.trim() : "";
  const name = typeof record.name === "string" ? record.name.trim() : "";
  if (!name) throw new Error("特殊人员姓名不能为空");
  return { employeeNo, name };
}

function normalizeSpecialPersonnel(value: unknown): SpecialPersonnelConfig {
  if (!value || typeof value !== "object") throw new Error("特殊人员配置格式错误");
  const record = value as Record<string, unknown>;
  const config = {} as SpecialPersonnelConfig;
  const configuredGroups = new Map<string, Set<SpecialGroup>>();
  for (const group of SPECIAL_GROUPS) {
    const rawPeople = record[group] ?? [];
    if (!Array.isArray(rawPeople)) throw new Error("特殊人员配置缺少分组");
    config[group] = rawPeople.map(normalizePerson);
    for (const person of config[group]) {
      const key = personKey(person);
      const groups = configuredGroups.get(key) ?? new Set<SpecialGroup>();
      if (groups.has(group)) {
        throw new Error(person.employeeNo ? `工号 ${person.employeeNo} 不能重复配置` : `${person.name} 不能重复配置`);
      }
      groups.add(group);
      if (groups.size > 1 && !isAllowedSpecialOverlap(groups)) {
        throw new Error(person.employeeNo ? `工号 ${person.employeeNo} 不能重复配置` : `${person.name} 不能重复配置`);
      }
      configuredGroups.set(key, groups);
    }
  }
  return config;
}

function normalizeSettings(value: unknown): AttendanceSettings {
  if (!value || typeof value !== "object") throw new Error("设置文件格式错误");
  const record = value as Record<string, unknown>;
  const specialPersonnel = normalizeSpecialPersonnel(
    record.specialPersonnel && typeof record.specialPersonnel === "object"
      ? record.specialPersonnel
      : value
  );
  const rawExcluded = record.excludedPersonnel ?? [];
  if (!Array.isArray(rawExcluded)) throw new Error("不参与考勤人员配置格式错误");
  const excludedPersonnel = rawExcluded.map(normalizePerson);
  const specialPeople = new Set(
    SPECIAL_GROUPS.flatMap((group) => specialPersonnel[group]).map(personKey)
  );
  const usedExcluded = new Set<string>();
  for (const person of excludedPersonnel) {
    const key = personKey(person);
    if (specialPeople.has(key) || usedExcluded.has(key)) {
      throw new Error(person.employeeNo ? `工号 ${person.employeeNo} 不能重复配置` : `${person.name} 不能重复配置`);
    }
    usedExcluded.add(key);
  }
  const rawStatutoryHolidayDates = record.statutoryHolidayDates ?? [];
  if (!Array.isArray(rawStatutoryHolidayDates)) throw new Error("三倍工资日配置格式错误");
  const statutoryHolidayDates = rawStatutoryHolidayDates.map((date) => {
    if (typeof date !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(date)) {
      throw new Error("三倍工资日必须使用 YYYY-MM-DD 格式");
    }
    const parsed = new Date(`${date}T00:00:00Z`);
    if (Number.isNaN(parsed.getTime()) || parsed.toISOString().slice(0, 10) !== date) {
      throw new Error(`三倍工资日 ${date} 不是有效日期`);
    }
    return date;
  });
  if (new Set(statutoryHolidayDates).size !== statutoryHolidayDates.length) {
    throw new Error("三倍工资日不能重复配置");
  }
  statutoryHolidayDates.sort();
  return { specialPersonnel, excludedPersonnel, statutoryHolidayDates };
}

async function readSettings(): Promise<AttendanceSettings> {
  try {
    const content = await fs.promises.readFile(settingsPath(), "utf8");
    return normalizeSettings(JSON.parse(content));
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code === "ENOENT") {
      try {
        const legacy = await fs.promises.readFile(legacySpecialPersonnelPath(), "utf8");
        return normalizeSettings(JSON.parse(legacy));
      } catch (legacyError) {
        if ((legacyError as NodeJS.ErrnoException).code === "ENOENT") {
          return emptySettings();
        }
        throw legacyError;
      }
    }
    throw error;
  }
}

async function writeSettings(value: unknown): Promise<AttendanceSettings> {
  const settings = normalizeSettings(value);
  const destination = settingsPath();
  const temporary = `${destination}.tmp`;
  await fs.promises.mkdir(path.dirname(destination), { recursive: true });
  await fs.promises.writeFile(temporary, `${JSON.stringify(settings, null, 2)}\n`, "utf8");
  await fs.promises.rename(temporary, destination);
  return settings;
}

async function createWindow(): Promise<void> {
  mainWindow = new BrowserWindow({
    width: 1000,
    height: 760,
    minWidth: 760,
    minHeight: 640,
    show: false,
    title: "福拉特考勤统计",
    icon: appIconPath(),
    backgroundColor: "#eaebf0",
    titleBarStyle: "hidden",
    titleBarOverlay: {
      color: "#eaebf0",
      symbolColor: "#747889",
      height: 44
    },
    webPreferences: {
      preload: path.join(__dirname, "../preload/index.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true
    }
  });
  mainWindow.webContents.setWindowOpenHandler(() => ({ action: "deny" }));
  mainWindow.webContents.on("will-navigate", (event) => event.preventDefault());
  mainWindow.once("ready-to-show", () => mainWindow?.show());
  if (!app.isPackaged && process.env.ELECTRON_RENDERER_URL) {
    await mainWindow.loadURL(process.env.ELECTRON_RENDERER_URL);
  } else {
    await mainWindow.loadFile(path.join(__dirname, "../renderer/index.html"));
  }
}

function registerIpc(): void {
  ipcMain.handle("attendance:select-input", async () => {
    const result = mainWindow
      ? await dialog.showOpenDialog(mainWindow, {
          title: "选择钉钉考勤报表",
          properties: ["openFile"],
          filters: [{ name: "Excel 工作簿", extensions: ["xlsx"] }]
        })
      : await dialog.showOpenDialog({
          title: "选择钉钉考勤报表",
          properties: ["openFile"],
          filters: [{ name: "Excel 工作簿", extensions: ["xlsx"] }]
        });
    if (result.canceled || result.filePaths.length === 0) return null;
    const selectedPath = result.filePaths[0];
    const stats = await fs.promises.stat(selectedPath);
    return { path: selectedPath, name: path.basename(selectedPath), size: stats.size };
  });

  ipcMain.handle("attendance:select-output", async (_event, defaultName: string) => {
    const result = mainWindow
      ? await dialog.showSaveDialog(mainWindow, {
          title: "保存考勤统计表",
          defaultPath: defaultName,
          filters: [{ name: "Excel 工作簿", extensions: ["xlsx"] }]
        })
      : await dialog.showSaveDialog({
          title: "保存考勤统计表",
          defaultPath: defaultName,
          filters: [{ name: "Excel 工作簿", extensions: ["xlsx"] }]
        });
    return result.canceled ? null : result.filePath;
  });

  ipcMain.handle("attendance:inspect", async (_event, inputPath: string): Promise<InspectResponse> => {
    const payload = JSON.parse(await getNativeApi().inspectDingtalk(inputPath)) as {
      period: { year: number; month: number };
      sheets: WorksheetSummary[];
      employees: Array<{ employee_no: string; name: string }>;
    };
    return {
      filename: path.basename(inputPath),
      sourcePath: inputPath,
      sheets: payload.sheets,
      year: payload.period.year,
      month: payload.period.month,
      employees: payload.employees.map<EmployeeIdentity>((employee) => ({
        employeeNo: employee.employee_no,
        name: employee.name
      }))
    };
  });

  ipcMain.handle("attendance:get-settings", () => readSettings());
  ipcMain.handle("attendance:import-settings", async () => {
    const result = mainWindow
      ? await dialog.showOpenDialog(mainWindow, {
          title: "导入考勤设置",
          properties: ["openFile"],
          filters: [{ name: "JSON 配置", extensions: ["json"] }]
        })
      : await dialog.showOpenDialog({
          title: "导入考勤设置",
          properties: ["openFile"],
          filters: [{ name: "JSON 配置", extensions: ["json"] }]
        });
    if (result.canceled || result.filePaths.length === 0) return null;
    const content = await fs.promises.readFile(result.filePaths[0], "utf8");
    return normalizeSettings(JSON.parse(content));
  });
  ipcMain.handle("attendance:export-settings", async (_event, value: unknown) => {
    const settings = normalizeSettings(value);
    const result = mainWindow
      ? await dialog.showSaveDialog(mainWindow, {
          title: "导出考勤设置",
          defaultPath: "考勤设置.json",
          filters: [{ name: "JSON 配置", extensions: ["json"] }]
        })
      : await dialog.showSaveDialog({
          title: "导出考勤设置",
          defaultPath: "考勤设置.json",
          filters: [{ name: "JSON 配置", extensions: ["json"] }]
        });
    if (result.canceled || !result.filePath) return false;
    await fs.promises.writeFile(result.filePath, `${JSON.stringify(settings, null, 2)}\n`, "utf8");
    return true;
  });
  ipcMain.handle("attendance:save-settings", (_event, settings: unknown) =>
    writeSettings(settings)
  );

  ipcMain.handle(
    "attendance:generate",
    async (
      _event,
      inputPath: string,
      outputPath: string
    ): Promise<GenerateResponse> => {
      if (path.resolve(inputPath) === path.resolve(outputPath)) {
        throw new Error("输出文件不能覆盖原始钉钉考勤报表");
      }
      const settings = await readSettings();
      const report = JSON.parse(
        await getNativeApi().generateReport(
          inputPath,
          outputPath,
          historyTemplatePath(),
          JSON.stringify(settings)
        )
      ) as NativeReport;
      return {
        filename: path.basename(outputPath),
        outputPath,
        summaryRows: report.summary_rows.length,
        exceptionRows: report.exception_rows.length,
        warnings: report.warnings
      };
    }
  );

  ipcMain.handle("attendance:reveal", async (_event, outputPath: string) => {
    shell.showItemInFolder(outputPath);
  });
}

if (squirrelStartup) app.quit();
else {
  app.whenReady().then(async () => {
    nativeTheme.themeSource = "light";
    if (process.platform === "darwin") app.dock?.setIcon(appIconPath());
    registerIpc();
    await createWindow();
    app.on("activate", () => {
      if (BrowserWindow.getAllWindows().length === 0) void createWindow();
    });
  });
  app.on("window-all-closed", () => {
    if (process.platform !== "darwin") app.quit();
  });
}
