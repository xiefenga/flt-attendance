import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { app, BrowserWindow, dialog, ipcMain, shell } from "electron";
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

const DEFAULT_SETTINGS: AttendanceSettings = {
  specialPersonnel: {
    punchMealNoOvertime: [
      { employeeNo: "22017", name: "袁亚鸿" },
      { employeeNo: "22018", name: "刘苏伟" },
      { employeeNo: "23035", name: "郑立明" },
      { employeeNo: "23041", name: "王善鹤" },
      { employeeNo: "23095", name: "吴通" },
      { employeeNo: "26265", name: "刘景超" },
      { employeeNo: "26266", name: "李培丞" },
      { employeeNo: "", name: "吴晶晶" }
    ],
    noPunchMealNoOvertime: [
      { employeeNo: "25196", name: "李阳" },
      { employeeNo: "25182", name: "欧智元" },
      { employeeNo: "24135", name: "张丽雯" }
    ],
    noMealNoOvertime: [
      { employeeNo: "17003", name: "李欣" },
      { employeeNo: "24151", name: "李述华" },
      { employeeNo: "24168", name: "晁伟" },
      { employeeNo: "26281", name: "陈冬冬" }
    ],
    flexibleArrivalShift: [
      { employeeNo: "26333", name: "张一成" }
    ]
  },
  excludedPersonnel: []
};

const SPECIAL_GROUPS = [
  "punchMealNoOvertime",
  "noPunchMealNoOvertime",
  "noMealNoOvertime",
  "flexibleArrivalShift"
] as const;

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
  const employeeNumbers = new Set<string>();
  const namesWithoutNumber = new Set<string>();
  for (const group of SPECIAL_GROUPS) {
    const rawPeople = record[group] ??
      (group === "flexibleArrivalShift" ? DEFAULT_SETTINGS.specialPersonnel[group] : undefined);
    if (!Array.isArray(rawPeople)) throw new Error("特殊人员配置缺少分组");
    config[group] = rawPeople.map(normalizePerson);
    for (const person of config[group]) {
      if (person.employeeNo) {
        if (employeeNumbers.has(person.employeeNo)) {
          throw new Error(`工号 ${person.employeeNo} 不能重复配置`);
        }
        employeeNumbers.add(person.employeeNo);
      } else {
        if (namesWithoutNumber.has(person.name)) {
          throw new Error(`${person.name} 不能重复配置`);
        }
        namesWithoutNumber.add(person.name);
      }
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
  const used = new Set<string>();
  for (const person of [
    ...SPECIAL_GROUPS.flatMap((group) => specialPersonnel[group]),
    ...excludedPersonnel
  ]) {
    const key = person.employeeNo ? `employee:${person.employeeNo}` : `name:${person.name}`;
    if (used.has(key)) {
      throw new Error(person.employeeNo ? `工号 ${person.employeeNo} 不能重复配置` : `${person.name} 不能重复配置`);
    }
    used.add(key);
  }
  return { specialPersonnel, excludedPersonnel };
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
          return structuredClone(DEFAULT_SETTINGS);
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
  const isMac = process.platform === "darwin";
  const isWindows = process.platform === "win32";

  mainWindow = new BrowserWindow({
    width: 1000,
    height: 760,
    minWidth: 760,
    minHeight: 640,
    show: false,
    title: "福拉特考勤统计",
    backgroundColor: isMac || isWindows ? "#00000000" : "#eaebf0",
    transparent: isMac,
    ...(isMac
      ? {
          vibrancy: "sidebar" as const,
          visualEffectState: "followWindow" as const
        }
      : {}),
    ...(isWindows ? { backgroundMaterial: "acrylic" as const } : {}),
    titleBarStyle: "hidden",
    titleBarOverlay: {
      color: isMac || isWindows ? "#00000000" : "#f4f4f7",
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
  ipcMain.handle("attendance:get-default-settings", () => structuredClone(DEFAULT_SETTINGS));
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
