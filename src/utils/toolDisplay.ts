export function normalizeToolName(name: string) {
  const raw = name.trim()
  if (!raw) return raw

  if (raw.startsWith('workspace_') || raw.startsWith('skills_')) return raw

  const mcpPrefixMatch = raw.match(/^mcp__[^_]+__(.+)$/)
  if (mcpPrefixMatch && mcpPrefixMatch[1]) {
    return `mcp__${mcpPrefixMatch[1]}`
  }

  return raw
}

export function renderToolLabel(name: string) {
  const raw = name.trim()
  const schedulerLabels: Record<string, string> = {
    scheduler_jobs_list: 'scheduler.list_jobs',
    scheduler_job_create: 'scheduler.create_job',
    scheduler_job_update: 'scheduler.update_job',
    scheduler_job_delete: 'scheduler.delete_job',
    scheduler_job_run: 'scheduler.run_job',
    scheduler_runs_list: 'scheduler.list_runs'
  }
  if (schedulerLabels[raw]) return schedulerLabels[raw]
  if (!raw) return '未知工具'

  if (raw.startsWith('mcp__')) {
    const parts = raw.split('__')
    if (parts.length >= 3) {
      const server = parts[1] || 'mcp'
      const tool = parts.slice(2).join('__') || 'tool'
      return `${server}.${tool}`
    }
  }

  return raw
}

export function truncateMiddle(input: string, max = 64) {
  if (input.length <= max) return input
  const head = Math.ceil((max - 1) / 2)
  const tail = Math.floor((max - 1) / 2)
  return `${input.slice(0, head)}...${input.slice(input.length - tail)}`
}

export function parseJsonRecord(raw: string): Record<string, unknown> | null {
  const trimmed = raw.trim()
  if (!trimmed) return null

  try {
    const parsed = JSON.parse(trimmed)
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return parsed as Record<string, unknown>
    }
  } catch {
    return null
  }

  return null
}

export function formatToolPayload(raw: string, limit = 12_000) {
  const trimmed = raw.trim()
  if (!trimmed) return ''

  let formatted = trimmed
  try {
    const parsed = JSON.parse(trimmed)
    formatted = JSON.stringify(parsed, null, 2)
  } catch {
    // keep raw text when payload is not valid JSON
  }

  if (formatted.length <= limit) return formatted
  return `${formatted.slice(0, limit)}\n...(输出过长，已截断)`
}

export function extractJsonLikeStringField(raw: string, fieldName: string) {
  const input = raw.trim()
  if (!input) return ''

  const escapedField = fieldName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const pattern = new RegExp(`"${escapedField}"\\s*:\\s*"([^"]*)`)
  const match = input.match(pattern)
  if (!match || !match[1]) return ''

  return decodeJsonLikeString(match[1]).trim()
}

export function decodeJsonLikeString(value: string) {
  return value
    .replace(/\\\\/g, '\\')
    .replace(/\\"/g, '"')
    .replace(/\\n/g, '\n')
    .replace(/\\r/g, '\r')
    .replace(/\\t/g, '\t')
}

