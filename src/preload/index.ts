import { contextBridge, ipcRenderer } from "electron";

import type { AttendanceDesktopApi } from "../shared/ipc-contract";

const attendanceDesktop: AttendanceDesktopApi = {
  selectInput: () => ipcRenderer.invoke("attendance:select-input"),
  selectOutput: (defaultName) => ipcRenderer.invoke("attendance:select-output", defaultName),
  inspect: (inputPath) => ipcRenderer.invoke("attendance:inspect", inputPath),
  getSettings: () => ipcRenderer.invoke("attendance:get-settings"),
  importSettings: () => ipcRenderer.invoke("attendance:import-settings"),
  exportSettings: (settings) => ipcRenderer.invoke("attendance:export-settings", settings),
  saveSettings: (settings) => ipcRenderer.invoke("attendance:save-settings", settings),
  generate: (inputPath, outputPath) =>
    ipcRenderer.invoke("attendance:generate", inputPath, outputPath),
  reveal: (outputPath) => ipcRenderer.invoke("attendance:reveal", outputPath)
};

contextBridge.exposeInMainWorld("attendanceDesktop", attendanceDesktop);
