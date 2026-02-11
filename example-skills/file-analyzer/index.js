#!/usr/bin/env node

/**
 * PETool Skill - File Analyzer
 *
 * Analyzes code files and provides statistics like:
 * - Line count
 * - Function/class count
 * - Import statements
 * - TODO/FIXME comments
 */

const fs = require('fs');
const path = require('path');

// Parse input parameters
const paramsJson = process.argv[2] || '{}';
const params = JSON.parse(paramsJson);

const filePath = params.file_path;

if (!filePath) {
  console.error(JSON.stringify({ error: 'file_path parameter is required' }));
  process.exit(1);
}

try {
  const content = fs.readFileSync(filePath, 'utf-8');
  const ext = path.extname(filePath);
  const lines = content.split('\n');

  // Basic statistics
  const stats = {
    file_path: filePath,
    extension: ext,
    total_lines: lines.length,
    non_empty_lines: lines.filter(l => l.trim()).length,
    characters: content.length,
    size_bytes: Buffer.byteLength(content, 'utf8')
  };

  // Language-specific analysis
  if (['.js', '.ts', '.jsx', '.tsx'].includes(ext)) {
    stats.imports = content.match(/^import\s+/gm)?.length || 0;
    stats.exports = content.match(/^export\s+/gm)?.length || 0;
    stats.functions = content.match(/function\s+\w+|=>\s*{|^\s*\w+\s*\(.*\)\s*{/gm)?.length || 0;
  } else if (['.rs'].includes(ext)) {
    stats.functions = content.match(/fn\s+\w+/g)?.length || 0;
    stats.structs = content.match(/struct\s+\w+/g)?.length || 0;
    stats.impls = content.match(/impl\s+\w+/g)?.length || 0;
  } else if (['.py'].includes(ext)) {
    stats.imports = content.match(/^import\s+|^from\s+/gm)?.length || 0;
    stats.functions = content.match(/def\s+\w+/g)?.length || 0;
    stats.classes = content.match(/^class\s+\w+/gm)?.length || 0;
  }

  // Find TODO/FIXME comments
  const todos = content.match(/\/\/\s*(TODO|FIXME):.*$/gm) || [];
  stats.todos = todos.map(t => t.trim());

  const result = {
    success: true,
    stats,
    analyzed_at: new Date().toISOString()
  };

  console.log(JSON.stringify(result, null, 2));

} catch (error) {
  console.error(JSON.stringify({
    success: false,
    error: error.message
  }));
  process.exit(1);
}
