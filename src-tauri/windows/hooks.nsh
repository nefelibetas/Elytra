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
  IfSilent elytra_game_data_done
  ; 卸载前告知：安装目录内的游戏数据不会被删除（非递归 RMDir 删不掉非空目录）
  IfFileExists "$INSTDIR\.minecraft\*.*" 0 elytra_game_data_done
    MessageBox MB_OK|MB_ICONINFORMATION "检测到安装目录内存在游戏数据：$\r$\n$INSTDIR\.minecraft$\r$\n$\r$\n卸载不会删除它（避免误删整合包、存档等）。若你不再需要，请手动删除该目录。"
  elytra_game_data_done:
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  ; 「删除应用数据」复选框：用户明确要求清理配置，按本项目实际路径删除。
  ; 与 Tauri 原生行为并集（原路径 xyz.bridgefish.elytra 一并清掉，防止旧版本残留）。
  ${If} $DeleteAppDataCheckboxState = 1
  ${AndIf} $UpdateMode <> 1
    RmDir /r "$APPDATA\Elytra"
    RmDir /r "$LOCALAPPDATA\Elytra"
    RmDir /r "$APPDATA\${BUNDLEID}"
    RmDir /r "$LOCALAPPDATA\${BUNDLEID}"
  ${EndIf}
!macroend
