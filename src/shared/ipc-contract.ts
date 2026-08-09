export interface DesktopSelection {
  path: string;
  name: string;
  size: number;
}

export interface WorksheetSummary {
  name: string;
  rows: number;
  columns: number;
  data_rows: number;
  unique_employees: number;
}

export interface EmployeeIdentity {
  employeeNo: string;
  name: string;
}

export type SpecialPersonnelGroup =
  | "punchMealNoOvertime"
  | "noPunchMealNoOvertime"
  | "noMealNoOvertime"
  | "flexibleArrivalShift"
  | "sixDayNoMeal"
  | "sixDayFourHourNoMeal";

export interface SpecialPerson {
  employeeNo: string;
  name: string;
}

export interface SpecialPersonnelConfig {
  punchMealNoOvertime: SpecialPerson[];
  noPunchMealNoOvertime: SpecialPerson[];
  noMealNoOvertime: SpecialPerson[];
  flexibleArrivalShift: SpecialPerson[];
  sixDayNoMeal: SpecialPerson[];
  sixDayFourHourNoMeal: SpecialPerson[];
}

export interface AttendanceSettings {
  specialPersonnel: SpecialPersonnelConfig;
  excludedPersonnel: SpecialPerson[];
}

export interface InspectResponse {
  filename: string;
  sourcePath: string;
  sheets: WorksheetSummary[];
  year: number;
  month: number;
  employees: EmployeeIdentity[];
}

export interface GenerateResponse {
  filename: string;
  outputPath: string;
  summaryRows: number;
  exceptionRows: number;
  warnings: string[];
}

export interface AttendanceDesktopApi {
  selectInput(): Promise<DesktopSelection | null>;
  selectOutput(defaultName: string): Promise<string | null>;
  inspect(inputPath: string): Promise<InspectResponse>;
  getSettings(): Promise<AttendanceSettings>;
  importSettings(): Promise<AttendanceSettings | null>;
  exportSettings(settings: AttendanceSettings): Promise<boolean>;
  saveSettings(settings: AttendanceSettings): Promise<AttendanceSettings>;
  generate(
    inputPath: string,
    outputPath: string
  ): Promise<GenerateResponse>;
  reveal(outputPath: string): Promise<void>;
}
