// 配置损坏恢复提示：启动期拉取（后端 setup 读配置时可能已恢复，事件会早于前端就绪而丢失）
// + 运行期订阅事件（用户操作触发保存时发生的恢复）。两条通道天然去重：
// 启动期那次事件无人监听、只剩在通知槽里被拉走；运行期那次无人再拉取、只走事件。
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { showError } from "../app/alert";
import { CONFIG_RECOVERY, type RecoveryNotice } from "../app/events";

function notify(n: RecoveryNotice) {
  showError(
    `配置文件已损坏，已重置为默认配置。\n原文件已备份到：${n.backup_path ?? "（备份失败）"}\n原因：${n.reason}`,
    true,
  );
}

let watching = false;

/** 幂等：应用启动时调用一次。 */
export async function ensureConfigRecoveryWatch(): Promise<void> {
  if (watching) return;
  watching = true;
  await listen<RecoveryNotice>(CONFIG_RECOVERY, (e) => notify(e.payload));
  const pending = await invoke<RecoveryNotice | null>("take_config_recovery_notice");
  if (pending) notify(pending);
}
