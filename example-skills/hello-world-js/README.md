# Hello World Skill (JavaScript)

A simple example skill for PETool that demonstrates the basic structure of a JavaScript-based skill.

## Installation

```bash
# Clone this skill to your PETool skills directory
# Or use the PETool UI to install from: https://github.com/yourusername/hello-world-js-skill
```

## Usage

The skill accepts a `name` parameter:

```json
{
  "name": "PETool User"
}
```

## Output

```json
{
  "success": true,
  "message": "Hello, PETool User!",
  "timestamp": "2024-01-01T00:00:00.000Z",
  "skill": {
    "id": "hello-world-js",
    "name": "Hello World (JavaScript)",
    "version": "1.0.0"
  }
}
```

## Skill Structure

- `skill.json` - Skill metadata
- `index.js` - Main skill entry point
- `README.md` - This file
