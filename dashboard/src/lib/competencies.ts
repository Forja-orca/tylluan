export function parseCompetencies(raw: unknown): Record<string, number> {
  if (!raw) return {};
  if (typeof raw === 'object' && !Array.isArray(raw)) {
    const res: Record<string, number> = {};
    for (const [k, v] of Object.entries(raw as Record<string, unknown>)) {
      if (typeof v === 'number') {
        res[k] = v;
      } else if (typeof v === 'string') {
        const num = parseFloat(v);
        if (!isNaN(num)) res[k] = num;
      }
    }
    return res;
  }
  if (typeof raw === 'string') {
    try {
      const parsed = JSON.parse(raw);
      return parseCompetencies(parsed);
    } catch {
      return {};
    }
  }
  return {};
}
