# Professor Application Tracker (Dioxus edition)

The same tracker as the egui version, with the interface rebuilt in
[Dioxus](https://dioxuslabs.com) (Rust, desktop) and styled with CSS: cards, soft
shadows, gradient accents, status pills, colour-coded deadlines, blurred modal
dialogs and a hover-linked pie chart. **No features were added or removed.**

## What it does

- Add, edit and delete applications (delete asks for confirmation)
- Five search boxes (professor, email, university, program, country) with
  typo-tolerant fuzzy matching, plus Status and Response filters
- Click a column header to sort (default: soonest deadline first)
- Deadline reminders: overdue / due today / due within 7 days are colour-coded in
  the table, summarised in the header, and listed in a Reminders dialog that opens
  on launch when something needs attention (Edit and Mark done buttons)
- CSV export of the current view or of everything
- Pie chart of applications per country; hover a slice or a legend row to highlight it
- "Response (yes/total)" counter in the table header

## Requirements

- Rust 1.88 or newer
- A C compiler (SQLite is compiled from source and bundled)
- **Linux:** the WebKitGTK development packages. On Debian/Ubuntu:

  ```bash
  sudo apt install build-essential pkg-config libwebkit2gtk-4.1-dev libgtk-3-dev \
       libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
  ```
- **macOS:** nothing extra (uses the system WebView)
- **Windows:** the WebView2 runtime (already present on current Windows 10/11)

## Run

```bash
cargo run --release
```

The first build compiles Dioxus and its WebView stack from source and takes a
while (about 8 minutes on a single-core machine). Later builds are fast.

## Your data

Identical to the egui version: the database lives in a per-user data folder (no
root, no `chmod 777`), or at `PROFESSOR_TRACKER_DB` if you set it. The table layout
matches the original Python app and the egui version, so one database file works
with all three.

| Platform | Default database location |
|---|---|
| Linux | `~/.local/share/professor-tracker/professor_applications.db` |
| macOS | `~/Library/Application Support/professor-tracker/professor_applications.db` |
| Windows | `%APPDATA%\professor-tracker\data\professor_applications.db` |

CSV exports go to your Documents folder (home folder if there is none) as
`professor_applications_YYYYMMDD_HHMMSS.csv`; existing files are never overwritten.

## Security notes

- 100% safe Rust (`#![forbid(unsafe_code)]`); the app makes no network requests and
  the stylesheet is embedded, so no web fonts or scripts are loaded
- Every SQL statement uses bound parameters
- All input is trimmed and validated before it reaches the database
- Text is rendered through Dioxus, which escapes it, so a name or email body
  containing HTML or script is shown as plain text
- CSV cells starting with `=`, `+`, `-` or `@` get a leading `'` (CSV injection)
- On Unix the data folder is `0700` and the database and exports are `0600`
- The database is not encrypted; use full-disk encryption and keep `*.db` out of git

## Project layout

```
assets/style.css   all styling (colours are CSS variables at the top)
src/
  main.rs       window, icon, embeds the stylesheet
  ui.rs         Dioxus components: header, toolbar, table, chart, dialogs
  store.rs      shared state (signals) and actions: save, delete, export...
  model.rs  db.rs  search.rs  reminders.rs  export.rs  sort.rs  stats.rs  error.rs
                the logic modules, unchanged from the egui version
```

Only `ui.rs`, `store.rs`, `main.rs` and the stylesheet differ from the egui
version. To restyle the app, edit `assets/style.css` and rebuild.

## Notes on how inputs work

Text boxes are "uncontrolled": Dioxus sets their starting text once and never
rewrites it while you type. (Rewriting it on every keystroke drops characters when
you type quickly.) "Clear filters" empties the search boxes by remounting them.

## Tests

```bash
cargo test
```

The unit tests cover the logic modules (validation, database, search, reminders,
export, sorting, chart grouping).
