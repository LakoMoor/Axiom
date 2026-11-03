<!-- axiom skill reference -->
# Connection and recovery

| Symptom | What to do |
|---|---|
| No `axiom` tools in the app | `axiom-cli doctor`; if the registration is missing or stale, `axiom-cli setup --client <id> --dry-run`, show the plan, then apply with `--yes` after the user agrees |
| Registration looks right but no tools | Restart or reconnect the app; it reads its config at start-up |
| `axiom-cli` not found from a shell | The program isn't on `PATH`; registrations use its absolute path, so the app may still work |
| The app asks to approve each tool | That's the app's own policy; don't change it without the user |
| A tool says an engine is missing | Follow its remediation (Settings → Engines in the desktop app) |

Setup only ever adds or updates the `axiom` entry. It keeps comments and every other server,
writes a `.axiom.backup` copy of the original file first (never overwriting an older backup),
and puts the original back if the result doesn't read back correctly. `axiom-cli uninstall`
removes only entries and skill files that Axiom itself created.

Command-line fallback when no app is connected: every tool is a subcommand, for example
`axiom-cli triage /path/to/program` or
`axiom-cli core-target-open --json '{"path": "/path/to/program"}'`.
