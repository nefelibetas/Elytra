; NSIS 安装/卸载挂钩（Tauri bundle.windows.nsis.installerHooks）
;
; 背景——Tauri 默认卸载行为（见 tauri-bundler 的 installer.nsi 的 Section Uninstall）：
;   1) 逐文件 Delete 已知产物，最后 `RMDir "$INSTDIR"`（**非递归**，只删空目录）；
;   2) `$DeleteAppDataCheckboxState = 1` 时 `RmDir /r "$APPDATA\${BUNDLEID}"`。
;
; 对本项目的两点影响：
;   - 游戏目录（$INSTDIR\.minecraft）在安装目录内，而 RMDir 非递归 → 目录非空即保留，
;     游戏数据天然安全。但用户不知道这一点，容易以为卸载会把整合包一起删掉，故做告知。
;   - ${BUNDLEID} 是 identifier（xyz.bridgefish.elytra），而本项目配置实际写在
;     `%APPDATA%\Elytra`（见 util::dirs::config_dir 与 constants::CONFIG_DIR）——
;     两者不一致，于是「删除应用数据」复选框删的是一个并不存在的目录，形同虚设。
;     这里按实际路径补删，让复选框名副其实。

!macro NSIS_HOOK_PREUNINSTALL
  ; 静默卸载（/S：CI、脚本、批量运维）不弹窗，避免阻塞
  IfSilent elytra_no_note

  ; 卸载说明：把「删什么 / 不删什么」写清楚。
  ; 用户明确要求游戏数据绝不随卸载删除——这在机制上本已成立（非递归 RMDir 删不掉非空
  ; 目录），但必须显式告知，否则用户看到 .minecraft 还在会以为卸载没成功。
  IfFileExists "$INSTDIR\.minecraft\*.*" 0 elytra_no_game_data
    MessageBox MB_OK|MB_ICONINFORMATION "卸载说明$\r$\n$\r$\n【不会删除】游戏数据$\r$\n    $INSTDIR\.minecraft（整合包、存档、模组等）$\r$\n将被完整保留，需你自行清理或迁移。$\r$\n$\r$\n【会被清理】启动器配置与数据$\r$\n    %APPDATA%\Elytra（设置、账号凭证、任务记录、日志）$\r$\n仅在勾选「删除应用数据」时清除。"
    Goto elytra_no_note
  elytra_no_game_data:
    MessageBox MB_OK|MB_ICONINFORMATION "卸载说明$\r$\n$\r$\n本卸载不会删除任何游戏数据。$\r$\n$\r$\n【会被清理】启动器配置与数据$\r$\n    %APPDATA%\Elytra（设置、账号凭证、任务记录、日志）$\r$\n仅在勾选「删除应用数据」时清除。"
  elytra_no_note:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; 「删除应用数据」复选框：用户明确要求清理配置时，按本项目**实际的**路径删除。
  ; Tauri 原生删的是 `$APPDATA\${BUNDLEID}`（= identifier `xyz.bridgefish.elytra`），
  ; 而本项目配置写在 `%APPDATA%\Elytra`（constants::CONFIG_DIR），两者不一致，
  ; 故原生行为形同虚设，这里补上真实路径。
  ; 注：`%LOCALAPPDATA%` 下没有本项目数据（该环境变量仅用于 JDK 扫描），故不处理。
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    RmDir /r "$APPDATA\Elytra"
  ${EndIf}
!macroend
