import { useState } from "react";
import {
  BookOpenText,
  CalendarDays,
  Check,
  CircleCheck,
  ChevronLeft,
  ChevronRight,
  FolderOpen,
  Settings,
  Trash2,
  Upload,
  UserPlus
} from "lucide-react";
import { Markdown } from "@tanstack/markdown/react";

import { Button } from "@/components/ui/button";
import { EmployeePickerDialog } from "@/components/employee-picker-dialog";
import { Input } from "@/components/ui/input";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/select";
import currentAttendanceRules from "../../../docs/当前考勤规则.md?raw";

import type {
  AttendanceSettings,
  DesktopSelection,
  EmployeeIdentity,
  GenerateResponse,
  InspectResponse,
  SpecialPersonnelConfig,
  SpecialPersonnelGroup,
  SpecialPerson
} from "../../shared/ipc-contract";

type BusyState = "idle" | "validating" | "generating";
type SettingsPage = "home" | "special" | "excluded" | "holidays";

const SPECIAL_GROUPS: Array<{
  key: SpecialPersonnelGroup;
  title: string;
}> = [
  {
    key: "punchMealNoOvertime",
    title: "按打卡计算餐补"
  },
  {
    key: "weekdayWeekendPunchMealHolidayOvertime",
    title: "工作日/周末无加班（法定节假日正常）"
  },
  {
    key: "noPunchMealNoOvertime",
    title: "不打卡但有餐补"
  },
  {
    key: "noMealNoOvertime",
    title: "无餐补、无加班"
  },
  {
    key: "flexibleArrivalShift",
    title: "弹性到岗（8:30 分界）"
  },
  {
    key: "sixDayNoMeal",
    title: "六天制（每日8小时、无餐补无加班）"
  },
  {
    key: "sixDayFourHourNoMeal",
    title: "六天制（每日4小时、无餐补无加班）"
  }
];

function cloneSpecialPersonnel(config: SpecialPersonnelConfig): SpecialPersonnelConfig {
  return {
    punchMealNoOvertime: config.punchMealNoOvertime.map((person) => ({ ...person })),
    weekdayWeekendPunchMealHolidayOvertime:
      config.weekdayWeekendPunchMealHolidayOvertime.map((person) => ({ ...person })),
    noPunchMealNoOvertime: config.noPunchMealNoOvertime.map((person) => ({ ...person })),
    noMealNoOvertime: config.noMealNoOvertime.map((person) => ({ ...person })),
    flexibleArrivalShift: config.flexibleArrivalShift.map((person) => ({ ...person })),
    sixDayNoMeal: config.sixDayNoMeal.map((person) => ({ ...person })),
    sixDayFourHourNoMeal: config.sixDayFourHourNoMeal.map((person) => ({ ...person }))
  };
}

function cloneSettings(settings: AttendanceSettings): AttendanceSettings {
  return {
    specialPersonnel: cloneSpecialPersonnel(settings.specialPersonnel),
    excludedPersonnel: settings.excludedPersonnel.map((person) => ({ ...person })),
    statutoryHolidayDates: [...settings.statutoryHolidayDates]
  };
}

function specialPersonnelCount(settings: AttendanceSettings | null): number {
  return settings
    ? SPECIAL_GROUPS.reduce(
        (total, group) => total + settings.specialPersonnel[group.key].length,
        0
      )
    : 0;
}

function matchesPerson(left: SpecialPerson, right: SpecialPerson): boolean {
  return left.employeeNo && right.employeeNo
    ? left.employeeNo === right.employeeNo
    : left.name === right.name;
}

function specialGroupsCanOverlap(
  left: SpecialPersonnelGroup,
  right: SpecialPersonnelGroup
): boolean {
  return (left === "punchMealNoOvertime" && right === "flexibleArrivalShift")
    || (left === "flexibleArrivalShift" && right === "punchMealNoOvertime");
}

function isAvailableForSpecialGroup(
  settings: AttendanceSettings,
  targetGroup: SpecialPersonnelGroup,
  person: SpecialPerson
): boolean {
  if (settings.excludedPersonnel.some((configured) => matchesPerson(configured, person))) {
    return false;
  }
  return !SPECIAL_GROUPS.some((group) =>
    !specialGroupsCanOverlap(targetGroup, group.key)
      && settings.specialPersonnel[group.key].some((configured) => matchesPerson(configured, person))
  );
}

function formatFileSize(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${Math.max(1, Math.round(bytes / 1024))} KB`;
}

function sheetValue(inspect: InspectResponse | null, name: string, field: "data_rows" | "unique_employees"): number {
  return inspect?.sheets.find((sheet) => sheet.name === name)?.[field] ?? 0;
}

function readableError(error: unknown): string {
  const message = error instanceof Error ? error.message : String(error);
  if (message.includes("缺少工作表")) return `${message}。请选择钉钉完整考勤报表。`;
  if (message.includes("缺少必需字段")) return `${message}。请检查原始表头。`;
  if (/not a zip|invalid|无法读取/i.test(message)) return "无法读取该文件，请确认它是未经修改的钉钉 .xlsx 考勤报表。";
  return message || "处理失败。";
}

export default function App() {
  const [file, setFile] = useState<DesktopSelection | null>(null);
  const [inspect, setInspect] = useState<InspectResponse | null>(null);
  const [busy, setBusy] = useState<BusyState>("idle");
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<GenerateResponse | null>(null);
  const [resultOpen, setResultOpen] = useState(false);
  const [rulesOpen, setRulesOpen] = useState(false);
  const [revealed, setRevealed] = useState(false);
  const [settingsDraft, setSettingsDraft] = useState<AttendanceSettings | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsPage, setSettingsPage] = useState<SettingsPage>("home");
  const [settingsBusy, setSettingsBusy] = useState(false);
  const [settingsError, setSettingsError] = useState<string | null>(null);
  const [settingsNotice, setSettingsNotice] = useState<string | null>(null);
  const [addGroup, setAddGroup] = useState<SpecialPersonnelGroup>("punchMealNoOvertime");
  const [employeePickerOpen, setEmployeePickerOpen] = useState(false);
  const [currentEmployeeIndex, setCurrentEmployeeIndex] = useState<string | undefined>();
  const [newStatutoryHolidayDate, setNewStatutoryHolidayDate] = useState("");

  const ready = file !== null && inspect !== null && busy === "idle";
  const availableEmployees = (inspect?.employees ?? [])
    .filter((employee) => {
      if (!settingsDraft) return true;
      const person = { employeeNo: employee.employeeNo, name: employee.name };
      if (settingsPage === "special") {
        return isAvailableForSpecialGroup(settingsDraft, addGroup, person);
      }
      return ![
        ...SPECIAL_GROUPS.flatMap((group) => settingsDraft.specialPersonnel[group.key]),
        ...settingsDraft.excludedPersonnel
      ].some((configured) => matchesPerson(configured, person));
    });
  const selectedEmployee = currentEmployeeIndex === undefined
    ? null
    : inspect?.employees[Number(currentEmployeeIndex)] ?? null;
  const employeePickerTarget = SPECIAL_GROUPS.find((group) => group.key === addGroup)?.title
    ?? "特殊人员";

  async function selectFile() {
    if (busy !== "idle") return;
    try {
      const selected = await window.attendanceDesktop.selectInput();
      if (!selected) return;
      setFile(selected);
      setInspect(null);
      setError(null);
      setResult(null);
      setBusy("validating");
      const inspected = await window.attendanceDesktop.inspect(selected.path);
      setInspect(inspected);
    } catch (caught) {
      setError(readableError(caught));
    } finally {
      setBusy("idle");
    }
  }

  async function generate() {
    if (!file || !inspect || !ready) return;
    const defaultName = `考勤统计表_${inspect.year}${String(inspect.month).padStart(2, "0")}.xlsx`;
    try {
      const outputPath = await window.attendanceDesktop.selectOutput(defaultName);
      if (!outputPath) return;
      setBusy("generating");
      setError(null);
      const generated = await window.attendanceDesktop.generate(file.path, outputPath);
      setResult(generated);
      setRevealed(false);
      setResultOpen(true);
    } catch (caught) {
      setError(readableError(caught));
    } finally {
      setBusy("idle");
    }
  }

  async function reveal() {
    if (!result) return;
    await window.attendanceDesktop.reveal(result.outputPath);
    setRevealed(true);
  }

  async function openSettings() {
    setSettingsBusy(true);
    setSettingsError(null);
    setSettingsNotice(null);
    try {
      const loaded = await window.attendanceDesktop.getSettings();
      setSettingsDraft(cloneSettings(loaded));
      setSettingsPage("home");
      setEmployeePickerOpen(false);
      setCurrentEmployeeIndex(undefined);
      setNewStatutoryHolidayDate("");
      setSettingsOpen(true);
    } catch (caught) {
      setError(readableError(caught));
    } finally {
      setSettingsBusy(false);
    }
  }

  function addConfiguredPeople(employees: EmployeeIdentity[]) {
    if (!settingsDraft || employees.length === 0) return;
    const people = employees
      .map<SpecialPerson>((employee) => ({
        employeeNo: employee.employeeNo.trim(),
        name: employee.name.trim()
      }))
      .filter((person, index, selected) => {
        return isAvailableForSpecialGroup(settingsDraft, addGroup, person)
          && selected.findIndex((existing) => matchesPerson(existing, person)) === index;
      });
    if (people.length === 0) {
      setSettingsError("所选人员已经配置");
      return;
    }
    setSettingsDraft({
      ...settingsDraft,
      specialPersonnel: {
        ...settingsDraft.specialPersonnel,
        [addGroup]: [...settingsDraft.specialPersonnel[addGroup], ...people]
      }
    });
    setSettingsError(null);
    setSettingsNotice(`已添加 ${people.length} 人，保存后生效`);
  }

  function addExcludedPerson() {
    if (!settingsDraft || !selectedEmployee) return;
    const person: SpecialPerson = {
      employeeNo: selectedEmployee.employeeNo.trim(),
      name: selectedEmployee.name.trim()
    };
    setSettingsDraft({
      ...settingsDraft,
      excludedPersonnel: [...settingsDraft.excludedPersonnel, person]
    });
    setCurrentEmployeeIndex(undefined);
    setSettingsError(null);
  }

  function removeSpecialPerson(group: SpecialPersonnelGroup, index: number) {
    if (!settingsDraft) return;
    setSettingsDraft({
      ...settingsDraft,
      specialPersonnel: {
        ...settingsDraft.specialPersonnel,
        [group]: settingsDraft.specialPersonnel[group].filter((_, itemIndex) => itemIndex !== index)
      }
    });
  }

  function removeExcludedPerson(index: number) {
    if (!settingsDraft) return;
    setSettingsDraft({
      ...settingsDraft,
      excludedPersonnel: settingsDraft.excludedPersonnel.filter((_, itemIndex) => itemIndex !== index)
    });
  }

  function addStatutoryHolidayDate() {
    if (!settingsDraft || !newStatutoryHolidayDate) return;
    if (settingsDraft.statutoryHolidayDates.includes(newStatutoryHolidayDate)) {
      setSettingsError(`${newStatutoryHolidayDate} 已经配置`);
      return;
    }
    setSettingsDraft({
      ...settingsDraft,
      statutoryHolidayDates: [...settingsDraft.statutoryHolidayDates, newStatutoryHolidayDate].sort()
    });
    setNewStatutoryHolidayDate("");
    setSettingsError(null);
  }

  function removeStatutoryHolidayDate(index: number) {
    if (!settingsDraft) return;
    setSettingsDraft({
      ...settingsDraft,
      statutoryHolidayDates: settingsDraft.statutoryHolidayDates.filter(
        (_, itemIndex) => itemIndex !== index
      )
    });
  }

  async function saveSettings() {
    if (!settingsDraft) return;
    setSettingsBusy(true);
    setSettingsError(null);
    try {
      await window.attendanceDesktop.saveSettings(settingsDraft);
      setSettingsOpen(false);
    } catch (caught) {
      setSettingsError(readableError(caught));
    } finally {
      setSettingsBusy(false);
    }
  }

  async function importSettings() {
    setSettingsError(null);
    setSettingsNotice(null);
    try {
      const imported = await window.attendanceDesktop.importSettings();
      if (imported) {
        setSettingsDraft(cloneSettings(imported));
        setSettingsNotice("配置已导入，保存后生效");
      }
    } catch (caught) {
      setSettingsError(readableError(caught));
    }
  }

  async function exportSettings() {
    if (!settingsDraft) return;
    setSettingsError(null);
    setSettingsNotice(null);
    try {
      const exported = await window.attendanceDesktop.exportSettings(settingsDraft);
      if (exported) setSettingsNotice("配置已导出");
    } catch (caught) {
      setSettingsError(readableError(caught));
    }
  }

  const people = sheetValue(inspect, "月度汇总", "unique_employees");
  const rawRows = sheetValue(inspect, "原始记录", "data_rows");
  const dailyRows = sheetValue(inspect, "每日统计", "data_rows");
  const buttonLabel =
    busy === "validating"
      ? "正在读取…"
      : busy === "generating"
        ? "正在生成…"
        : inspect
          ? "生成考勤统计表"
          : "请先选择报表";

  return (
    <div className="application-window">
      <header className="titlebar" aria-label="窗口标题栏" />
      <main>
        <section className="setup-view" aria-busy={busy !== "idle"}>
          <div className="intro">
            <h1>考勤统计</h1>
            <div className="intro-actions">
              <Button className="rules-button" type="button" onClick={() => setRulesOpen(true)}>
                <BookOpenText size={17} strokeWidth={1.8} /><span>考勤规则</span>
              </Button>
              <Button className="settings-button" type="button" aria-label="设置" title="设置" disabled={settingsBusy || busy !== "idle"} onClick={() => void openSettings()}>
                <span className="settings-icon" aria-hidden="true"><Settings size={20} strokeWidth={1.8} /></span>
              </Button>
            </div>
          </div>

          <div className={`file-panel${inspect ? " is-ready" : ""}${error ? " is-error" : ""}`}>
            <div className="file-copy">
              <div className="file-heading">
                <div>
                  <h2>钉钉考勤报表</h2>
                </div>
              </div>
              <div className="file-state" aria-live="polite">
                {file ? (
                  <>
                    <span className={error ? "status-icon is-error" : "status-icon"}>{error ? "!" : <CircleCheck size={18} strokeWidth={1.8} />}</span>
                    <div className="selected-copy">
                      <strong title={file.path}>{file.name}</strong>
                      <span>
                        {formatFileSize(file.size)}
                        {busy === "validating" ? " · 正在读取…" : inspect ? ` · ${people} 人` : ""}
                      </span>
                    </div>
                  </>
                ) : <span className="empty-state">尚未选择文件</span>}
              </div>
            </div>
            <Button className="choose-button" type="button" disabled={busy !== "idle"} onClick={() => void selectFile()}>
              <Upload size={18} strokeWidth={1.8} /><span>{file ? "更换文件" : "选择报表"}</span>
            </Button>
          </div>

          {inspect ? (
            <div className="validated-strip">
              <div><span>月份</span><strong>{inspect.year}年{inspect.month}月</strong></div>
              <div><span>员工</span><strong>{people}</strong></div>
              <div><span>原始记录</span><strong>{rawRows.toLocaleString()}</strong></div>
              <div><span>每日统计</span><strong>{dailyRows.toLocaleString()}</strong></div>
              <div className="validated-label"><CircleCheck size={18} strokeWidth={1.8} />读取完成</div>
            </div>
          ) : null}

          <div className="message-slot" aria-live="assertive">
            {error ? <div className="message"><span>!</span><div><strong>文件处理失败</strong><p>{error}</p></div></div> : null}
          </div>

          <div className="action-zone">
            <Button variant="primary" className="generate-button" type="button" disabled={!ready} onClick={() => void generate()}>
              {busy !== "idle" ? <span className="spinner" /> : null}<span>{buttonLabel}</span>
            </Button>
          </div>
          <footer className="version-footer">v{__APP_VERSION__}</footer>
        </section>
      </main>

      <Dialog open={rulesOpen} onOpenChange={setRulesOpen}>
        <DialogContent className="rules-dialog" aria-describedby={undefined}>
          <div className="rules-panel">
            <div className="rules-heading">
              <DialogTitle>当前考勤规则</DialogTitle>
            </div>
            <ScrollArea className="rules-scroll">
              <article className="rules-content">
                <Markdown>{currentAttendanceRules}</Markdown>
              </article>
            </ScrollArea>
          </div>
        </DialogContent>
      </Dialog>

      <Dialog open={settingsOpen} onOpenChange={setSettingsOpen}>
        <DialogContent className="settings-dialog" aria-describedby={undefined}>
        <div className="settings-panel">
          <div className="settings-heading">
            <div>
              {settingsPage !== "home" ? <Button variant="ghost" className="settings-back" type="button" onClick={() => setSettingsPage("home")}><ChevronLeft size={14} />返回设置</Button> : null}
              <DialogTitle className="settings-title">{settingsPage === "home" ? "设置" : settingsPage === "special" ? "特殊人员" : settingsPage === "excluded" ? "不参与考勤人员" : "三倍工资日"}</DialogTitle>
            </div>
            {settingsPage === "home" ? <div className="settings-tools">
              <Button variant="ghost" type="button" onClick={() => void importSettings()}>导入</Button>
              <Button variant="ghost" type="button" onClick={() => void exportSettings()}>导出</Button>
            </div> : null}
          </div>

          {settingsPage === "home" ? <div className="settings-menu">
            <Button variant="ghost" className="settings-menu-button" type="button" onClick={() => {
              setSettingsPage("special");
            }}>
              <strong>特殊人员</strong>
              <span className="settings-menu-count">{specialPersonnelCount(settingsDraft)}</span>
              <ChevronRight className="settings-menu-arrow" size={20} />
            </Button>
            <Button variant="ghost" className="settings-menu-button" type="button" onClick={() => {
              setNewStatutoryHolidayDate("");
              setSettingsPage("holidays");
            }}>
              <span className="settings-menu-label"><CalendarDays size={18} /><strong>三倍工资日</strong></span>
              <span className="settings-menu-count">{settingsDraft?.statutoryHolidayDates.length ?? 0}</span>
              <ChevronRight className="settings-menu-arrow" size={20} />
            </Button>
            <Button variant="ghost" className="settings-menu-button" type="button" onClick={() => {
              setCurrentEmployeeIndex(undefined);
              setSettingsPage("excluded");
            }}>
              <strong>不参与考勤人员</strong>
              <span className="settings-menu-count">{settingsDraft?.excludedPersonnel.length ?? 0}</span>
              <ChevronRight className="settings-menu-arrow" size={20} />
            </Button>
          </div> : null}

          {settingsPage === "special" ? <div className="settings-groups">
            {settingsDraft ? SPECIAL_GROUPS.map((group) => (
              <section className="settings-group" key={group.key}>
                <div className="settings-group-heading">
                  <h3>{group.title}</h3>
                  <span>{settingsDraft.specialPersonnel[group.key].length}</span>
                </div>
                <ScrollArea className="person-list">
                  {settingsDraft.specialPersonnel[group.key].length ? settingsDraft.specialPersonnel[group.key].map((person, index) => (
                    <div className="person-row" key={`${person.employeeNo}:${person.name}`}>
                      <div><strong>{person.name}</strong><span>{person.employeeNo || "工号待补"}</span></div>
                      <Button variant="dangerGhost" type="button" aria-label={`删除${person.name}`} title="删除" onClick={() => removeSpecialPerson(group.key, index)}><Trash2 size={14} /></Button>
                    </div>
                  )) : <span className="person-empty">暂无人员</span>}
                </ScrollArea>
              </section>
            )) : null}
          </div> : null}

          {settingsPage === "excluded" ? <div className="excluded-list settings-group">
            <div className="settings-group-heading">
              <h3>已排除人员</h3>
              <span>{settingsDraft?.excludedPersonnel.length ?? 0}</span>
            </div>
            <ScrollArea className="person-list">
              {settingsDraft?.excludedPersonnel.length ? settingsDraft.excludedPersonnel.map((person, index) => (
                <div className="person-row" key={`${person.employeeNo}:${person.name}`}>
                  <div><strong>{person.name}</strong><span>{person.employeeNo || "工号待补"}</span></div>
                  <Button variant="dangerGhost" type="button" aria-label={`删除${person.name}`} title="删除" onClick={() => removeExcludedPerson(index)}><Trash2 size={14} /></Button>
                </div>
              )) : <span className="person-empty">暂无人员</span>}
            </ScrollArea>
          </div> : null}

          {settingsPage === "holidays" ? <div className="holiday-settings">
            <div className="holiday-settings-note">
              <strong>按年份覆盖法定加班分类</strong>
              <p>某年添加任意日期后，该年只有清单中的日期计入法定加班（三倍工资），其他放假日计入周末加班。某年未添加日期时，继续沿用开源日历，将法定节假日连休的每一天都计入法定加班。</p>
            </div>
            <div className="settings-group holiday-list">
              <div className="settings-group-heading">
                <h3>已维护日期</h3>
                <span>{settingsDraft?.statutoryHolidayDates.length ?? 0}</span>
              </div>
              <ScrollArea className="person-list">
                {settingsDraft?.statutoryHolidayDates.length ? settingsDraft.statutoryHolidayDates.map((date, index) => (
                  <div className="person-row holiday-row" key={date}>
                    <div><strong>{date}</strong><span>{date.slice(0, 4)} 年三倍工资日</span></div>
                    <Button variant="dangerGhost" type="button" aria-label={`删除${date}`} title="删除" onClick={() => removeStatutoryHolidayDate(index)}><Trash2 size={14} /></Button>
                  </div>
                )) : <span className="person-empty">尚未维护，将沿用开源日历的现有分类</span>}
              </ScrollArea>
            </div>
            <div className="settings-add">
              <div className="settings-add-heading">
                <strong>添加三倍工资日</strong>
                <span>请选择实际法定节日当天</span>
              </div>
              <div className="holiday-add-fields">
                <Input type="date" aria-label="三倍工资日期" value={newStatutoryHolidayDate} onChange={(event) => setNewStatutoryHolidayDate(event.target.value)} />
                <Button type="button" disabled={!newStatutoryHolidayDate} onClick={addStatutoryHolidayDate}>添加</Button>
              </div>
            </div>
          </div> : null}

          {settingsPage !== "home" && settingsPage !== "holidays" ? <div className="settings-add">
            <div className="settings-add-heading">
              <strong>添加人员</strong>
              <span>{inspect ? "人员信息来自当前考勤报表" : "请先在首页选择考勤报表"}</span>
            </div>
            <div className={`settings-add-fields${settingsPage === "excluded" ? " is-excluded" : ""}`}>
              {settingsPage === "special" ? <Select value={addGroup} onValueChange={(value) => setAddGroup(value as SpecialPersonnelGroup)}>
                <SelectTrigger aria-label="添加到特殊人员分组"><SelectValue /></SelectTrigger>
                <SelectContent>{SPECIAL_GROUPS.map((group) => <SelectItem value={group.key} key={group.key}>{group.title}</SelectItem>)}</SelectContent>
              </Select> : null}
              {settingsPage === "special" ? <Button
                className="open-employee-picker"
                type="button"
                disabled={!inspect || availableEmployees.length === 0}
                onClick={() => setEmployeePickerOpen(true)}
              >
                <UserPlus size={16} />
                <span>{!inspect ? "请先选择考勤报表" : availableEmployees.length ? `选择人员（${availableEmployees.length}）` : "当前报表人员均已配置"}</span>
              </Button> : <>
                <Select value={currentEmployeeIndex} disabled={!inspect || availableEmployees.length === 0} onValueChange={setCurrentEmployeeIndex}>
                  <SelectTrigger aria-label="从当前报表选择员工"><SelectValue placeholder={!inspect ? "选择报表后可添加人员" : availableEmployees.length ? "选择人员" : "当前报表人员均已配置"} /></SelectTrigger>
                  <SelectContent>
                  {availableEmployees.map((employee, index) => (
                    <SelectItem value={String((inspect?.employees ?? []).indexOf(employee))} key={`${employee.employeeNo}:${employee.name}:${index}`}>{employee.name} · {employee.employeeNo || "无工号"}</SelectItem>
                  ))}
                  </SelectContent>
                </Select>
                <Button type="button" disabled={!selectedEmployee} onClick={addExcludedPerson}>添加</Button>
              </>}
            </div>
          </div> : null}

          <div className="settings-footer">
            <span className="settings-error" role="alert">{settingsError}</span>
            {!settingsError ? <span className="settings-notice">{settingsNotice}</span> : null}
            <Button type="button" onClick={() => setSettingsOpen(false)}>取消</Button>
            <Button variant="primary" type="button" disabled={settingsBusy} onClick={() => void saveSettings()}>{settingsBusy ? "保存中…" : "保存"}</Button>
          </div>
        </div>
        </DialogContent>
      </Dialog>

      <EmployeePickerDialog
        employees={availableEmployees}
        open={employeePickerOpen}
        targetLabel={employeePickerTarget}
        onOpenChange={setEmployeePickerOpen}
        onConfirm={addConfiguredPeople}
      />

      <Dialog open={resultOpen} onOpenChange={setResultOpen}>
        <DialogContent>
        <div className="result-panel">
          <span className="success-icon">✓</span>
          <div><DialogTitle>考勤统计表已生成</DialogTitle><DialogDescription>{result ? `已汇总 ${result.summaryRows} 人，并生成 ${result.exceptionRows} 人的异常明细。` : ""}</DialogDescription></div>
          {result ? <div className="output-file"><span>输出文件</span><strong title={result.outputPath}>{result.filename}</strong></div> : null}
          {result?.warnings.length ? <div className="warning-list"><strong>计算说明</strong>{result.warnings.map((warning) => <p key={warning}>• {warning}</p>)}</div> : null}
          <Button variant="primary" type="button" onClick={() => void reveal()}>{revealed ? <Check size={18} /> : <FolderOpen size={18} />}<span>{revealed ? "已在文件夹中定位" : "在文件夹中显示"}</span></Button>
        </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}
