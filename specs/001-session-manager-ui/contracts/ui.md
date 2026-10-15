# UI Contract

| Region | Content | Interactions |
|--------|---------|--------------|
| Left panel | Session titles, newest first, project name dimmed; "Refresh" button | Click → open session |
| Top of center | Single-line prompt input + Send; status ("running…") | Up/Down → history; Enter → send; disabled while running or no session |
| Center | Scrollable messages, "You"/"Claude" labels | Scroll |
| Right panel | One block per change: file path, red `-` lines, green `+` lines | Scroll |

Keyboard: Up/Down only act while the prompt input has focus.
