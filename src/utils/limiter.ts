// 并发上限队列：任务按提交顺序启动，最多 limit 个同时执行。
/** limit 为同时执行的任务数上限（下限 1）。 */
export function createLimiter(limit: number) {
  const max = Math.max(1, limit);
  let active = 0;
  const pending: (() => void)[] = [];

  const pump = () => {
    while (active < max) {
      const run = pending.shift();
      if (!run) return;
      active++;
      run();
    }
  };

  return <T>(task: () => Promise<T>): Promise<T> =>
    new Promise<T>((resolve, reject) => {
      pending.push(() => {
        void Promise.resolve()
          .then(task)
          .then(resolve, reject)
          .finally(() => {
            active--;
            pump();
          });
      });
      pump();
    });
}
