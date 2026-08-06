import type { AttendanceDesktopApi } from "../../shared/ipc-contract";

declare global {
  interface Window {
    attendanceDesktop: AttendanceDesktopApi;
  }
  const __APP_VERSION__: string;
}

export {};

