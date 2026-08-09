const path = require("node:path");

const projectRoot = __dirname;

module.exports = {
  packagerConfig: {
    asar: true,
    appBundleId: "com.flt.attendance",
    appCategoryType: "public.app-category.productivity",
    executableName: "flt-attendance",
    icon: path.join(projectRoot, "resources", "icons", "app-icon"),
    extraResource: [
      path.join(projectRoot, "resources", "native"),
      path.join(projectRoot, "resources", "icons"),
      path.join(projectRoot, "examples", "templates", "考勤统计表模板.xlsx")
    ],
    ignore: [/^\/(?!dist(?:\/|$)|package\.json$).+/]
  },
  rebuildConfig: {},
  makers: [
    {
      name: "@electron-forge/maker-squirrel",
      config: {
        name: "flt_attendance",
        authors: "FLT Attendance",
        description: "离线钉钉考勤统计工具",
        setupExe: "FLT Attendance Setup.exe",
        setupIcon: path.join(projectRoot, "resources", "icons", "app-icon.ico")
      }
    },
    { name: "@electron-forge/maker-dmg", config: { format: "ULFO" } },
    { name: "@electron-forge/maker-zip", platforms: ["darwin"] }
  ]
};
