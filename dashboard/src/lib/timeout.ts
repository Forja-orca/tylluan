/**
 * Wraps a promise with a timeout limit.
 * If the promise does not settle within `timeoutMs`, it rejects with an Error.
 * Automatically clears the timeout timer when the promise resolves or rejects.
 */
export function withTimeout<T>(
  promise: Promise<T>,
  timeoutMs = 10000,
  errorMsg = 'Request timed out after 10s. The endpoint took too long to respond.'
): Promise<T> {
  let timerId: ReturnType<typeof setTimeout>;

  const timeoutPromise = new Promise<T>((_, reject) => {
    timerId = setTimeout(() => {
      reject(new Error(errorMsg));
    }, timeoutMs);
  });

  return Promise.race([
    promise.then((res) => {
      clearTimeout(timerId);
      return res;
    }).catch((err) => {
      clearTimeout(timerId);
      throw err;
    }),
    timeoutPromise,
  ]);
}
