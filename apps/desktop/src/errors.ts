/** The message of a caught error, or a fallback when it carries none. */

export function errorMessage(error: unknown, fallback: string): string {
  return error instanceof Error ? error.message : fallback;
}
