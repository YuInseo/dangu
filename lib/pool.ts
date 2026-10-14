/** 동시에 n개씩만 돌린다. 수만 개 파일을 한꺼번에 열면 파일 핸들이 바닥난다. */
export async function pool<T>(items: readonly T[], n: number, fn: (item: T, index: number) => Promise<void>) {
  let next = 0;
  const workers = Array.from({ length: Math.max(1, Math.min(n, items.length)) }, async () => {
    while (next < items.length) {
      const i = next++;
      await fn(items[i]!, i);
    }
  });
  await Promise.all(workers);
}
