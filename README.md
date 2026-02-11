# PETool - AI Agent Desktop Application

A desktop AI agent application built with Tauri (Rust) and Vue 3, featuring a WeChat-style interface.

## Features

- **WeChat-style Interface**: Three-column layout (conversation list + chat panel + info panel)
- **Multiple LLM Support**: OpenAI (GPT-4, GPT-3.5), Claude, and OpenAI-compatible APIs
- **Streaming Responses**: Real-time chat with streaming AI responses
- **File Browser**: Integrated file explorer for project navigation
- **MCP Support**: Model Context Protocol for tool extensions
- **Skills System**: Plugin-based skill system for extensibility

## Prerequisites

- Node.js 18+
- Rust 1.70+
- pnpm/npm/yarn

## Installation

```bash
# Install dependencies
npm install

# Run development server
npm run dev

# Build for production
npm run build
```

## Development

```bash
# Start Tauri development mode
npm run tauri dev

# Build Tauri app
npm run tauri build
```

## Configuration

Configuration is stored in:
- **Windows**: `%APPDATA%\petool\config.json`
- **macOS**: `~/Library/Application Support/petool/config.json`
- **Linux**: `~/.config/petool/config.json`

### Configuration Options

```json
{
  "api_key": "your-api-key",
  "api_base": "https://api.openai.com/v1",
  "model": "gpt-4o-mini",
  "work_directory": "/path/to/project",
  "theme": "dark",
  "mcp_servers": []
}
```

## Project Structure

```
petool/
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── commands/       # Tauri commands
│   │   ├── services/       # Business logic
│   │   ├── models/         # Data models
│   │   └── utils/          # Utilities
│   └── Cargo.toml
└── src/                    # Vue frontend
    ├── components/         # Vue components
    ├── stores/            # Pinia stores
    └── App.vue
```

## License

MIT
