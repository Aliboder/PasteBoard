/** 全局 invoke 并发队列：限制同时进行的请求数，防止网格卡片批量渲染时的请求风暴 */
const MAX_CONCURRENT = 6;
let active = 0;
const queue: (() => void)[] = [];

function pump() {
  while (active < MAX_CONCURRENT && queue.length > 0) {
    const resolve = queue.shift()!;
    active += 1;
    resolve();
  }
}

/** 让 fn 在全局并发限制内执行 */
export async function throttled<T>(fn: () => Promise<T>): Promise<T> {
  await new Promise<void>((resolve) => {
    queue.push(resolve);
    pump();
  });
  try {
    return await fn();
  } finally {
    active -= 1;
    pump();
  }
}
