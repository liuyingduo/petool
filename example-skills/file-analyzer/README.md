# File Analyzer Skill

A PETool skill that analyzes code files and provides useful statistics.

## Features

- **Line count**: Total and non-empty lines
- **Size**: File size in bytes and characters
- **Language-specific stats**:
  - JavaScript/TypeScript: imports, exports, functions
  - Rust: functions, structs, impls
  - Python: imports, functions, classes
- **TODO/FIXME detection**: Finds all TODO and FIXME comments

## Usage

```json
{
  "file_path": "/path/to/your/code.js"
}
```

## Output Example

```json
{
  "success": true,
  "stats": {
    "file_path": "/path/to/your/code.js",
    "extension": ".js",
    "total_lines": 150,
    "non_empty_lines": 120,
    "characters": 4500,
    "size_bytes": 4500,
    "imports": 5,
    "exports": 3,
    "functions": 8,
    "todos": [
      "// TODO: Add error handling",
      "// FIXME: This function needs optimization"
    ]
  },
  "analyzed_at": "2024-01-01T00:00:00.000Z"
}
```

## Installation

1. Navigate to PETool settings → Skills → Browse Market
2. Search for "File Analyzer"
3. Click "Install"

Or install manually:

```bash
git clone https://github.com/yourusername/file-analyzer-skill
cp -r file-analyzer-skill ~/.config/petool/skills/
```
