// 有界模块级缓存：给长期运行会话里只增不减的 Record 缓存加容量上限。
// 策略：FIFO（容量满时淘汰最早写入的键）——无额外读取开销，不改变命中语义；
// 读取路径仍是 record[key]，仅写入需经 put()（已存在的键只更新值，不重复占位）。
import { reactive } from "vue";

/** 有界响应式 Record 缓存句柄。 */
export interface BoundedCache<V> {
  /** 缓存本体（响应式，调用方直接 record[key] 读取）。 */
  record: Record<string, V>;
  /** 写入；超出上限时淘汰最早写入的键。 */
  put(key: string, value: V): void;
  /** 删除单个键（失效重取用）。 */
  drop(key: string): void;
  /** 清空全部条目（版本切换等场景）。 */
  clear(): void;
}

/**
 * 创建容量上限为 max 的响应式 Record 缓存。
 * @param max 条目上限（≥ 同屏/同会话真正需要复用的键数，避免命中率下降）
 */
export function createBoundedCache<V>(max: number): BoundedCache<V> {
  const record = reactive<Record<string, V>>({});
  const keys: string[] = [];
  return {
    record,
    put(key, value) {
      if (!(key in record)) {
        keys.push(key);
        // 逐出最早写入的键（一次只增 1 个键，无需循环）
        if (keys.length > max) {
          const oldest = keys.shift();
          if (oldest !== undefined) delete record[oldest];
        }
      }
      record[key] = value;
    },
    drop(key) {
      delete record[key];
      const i = keys.indexOf(key);
      if (i >= 0) keys.splice(i, 1);
    },
    clear() {
      for (const k of keys) delete record[k];
      keys.length = 0;
    },
  };
}
