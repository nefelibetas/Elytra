// 有界模块级缓存：FIFO 上限，超限淘汰最早写入的键；写入必须经 put()，读取 record[key]。
import { reactive } from "vue";

/** 有界响应式 Record 缓存句柄。 */
export interface BoundedCache<V> {
  record: Record<string, V>;
  put(key: string, value: V): void;
  drop(key: string): void;
  clear(): void;
}

/** max 为条目上限，需 ≥ 同会话复用的键数（过小降低命中率）。 */
export function createBoundedCache<V>(max: number): BoundedCache<V> {
  const record = reactive<Record<string, V>>({});
  const keys: string[] = [];
  return {
    record,
    put(key, value) {
      if (!(key in record)) {
        keys.push(key);
        // 一次只增 1 个键，无需循环
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
