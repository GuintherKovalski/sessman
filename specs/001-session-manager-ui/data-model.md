# Data Model

## SessionMeta (sidebar entry)
- `id: String` file stem (session UUID)
- `path: PathBuf` transcript file
- `project: String` project directory name
- `title: String` `ai-title` → first prompt (truncated) → id
- `modified: SystemTime` file mtime, sort key (desc)

## Session (opened)
- `meta: SessionMeta`
- `cwd: Option<PathBuf>` from first event with `cwd`; used to run `claude`
- `messages: Vec<Message>`
- `prompts: Vec<String>` user-typed prompts, oldest first
- `changes: Vec<Change>`

## Message
- `role: User | Assistant`
- `text: String` text blocks joined; tool calls summarized as `[tool: Name]`

## Change
- `file: String`
- `old: String`, `new: String` (Write → old is empty)

## History (prompt-bar cursor)
- `entries: Vec<String>`, `pos: Option<usize>`, `draft: String`
- `up(current)`: first call saves `current` as draft, moves to newest; stops at oldest
- `down()`: moves newer; past newest returns draft and clears `pos`
