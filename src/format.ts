export function formatBytes(bytes: number | null): string {
  if (bytes === null || !Number.isFinite(bytes) || bytes < 0)
    return "Not available";
  if (bytes < 1024) return `${bytes} B`;
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), 4);
  return `${(bytes / 1024 ** exponent).toFixed(exponent === 1 ? 0 : 1)} ${["B", "KB", "MB", "GB", "TB"][exponent]}`;
}

export function formatDate(value: string | null): string {
  if (!value) return "Not available";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

export function pageRange(
  offset: number,
  limit: number,
  total: number,
): string {
  return total === 0
    ? "0 photos"
    : `${offset + 1}–${Math.min(offset + limit, total)} of ${total.toLocaleString()}`;
}
