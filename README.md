# Organizer – desktop app

A personal organizer in a ring-binder style. It's built with [Tauri 2](https://tauri.app): a small native shell (Rust) around the HTML/CSS/JS user interface in `src/index.html`.

## Features

* **Sections:** Year, Month, Week and Day planner pages, an address book, and notes with page templates (lined, dot grid, graph, checklist, Cornell, blank).
* **Your profile:** set up on first launch, and changeable any time via **File → My profile…** or the name button in the top bar. It is stored in the local database and will become your account when server sync is added.
* **Day time scale:** 60, 30 or 15 minutes (buttons on the Day page). Set the first and last time of day in **File → Settings…** (Ctrl/Cmd+,). Entries are kept when you switch scale.
* **Projects:** subject (customer or anyone else), group, status, start and due dates, description, people linked from the address book with their roles, and tasks with due dates and an assignee. The project list has search and can be grouped by group, subject or status. Deadlines appear in the calendar views, and each contact lists the projects they're in.
* **Languages:** English, Hrvatski (Croatian) and Deutsch (German). Choose one under **View → Language** or with the selector in the top bar. Day and month names come from the operating system's date formatting.
* **Menus:** File, Edit, View, Go and Help in the normal desktop menu bar. On a Mac they appear in the top bar of the screen. **Help → Keyboard shortcuts** lists every shortcut.
* **Printing (File → Print… or Ctrl/Cmd+P):** print the current pages, the address book, or blank template pages. Paper can be A4, A5 or Letter, with one page per sheet (portrait) or two pages per sheet (landscape). A5 with one page per sheet fits a standard A5 ring binder.
* **Links:** calendar events, projects, notes and people can be linked to each other, and a link shows up on both sides. Hover over an entry on the Day page and click the chain icon to link it. Projects, notes and contacts have a **Links** section, where you can also create a new event.
* **Attachments:** files, images and web links on projects, notes and contacts. Add them with the buttons, by dragging files onto the page, or by pasting an image (Ctrl/Cmd+V). Files open in their normal program. The desktop app copies each file into its own `attachments` folder (see the data folders below), so moving or deleting the original doesn't break the attachment.
* **Layout inspired by Organizer 6.1:** section tabs sit as index tabs along the top edge of the pages. Page-edge tabs show months in the calendar sections and A–Z in Contacts. Each page has a status bar at the bottom, the Calendar tab has a view selector in its status bar (Day planner, Week planner, Work week, Month, Year), and a toolbar sits at the top. A sidebar (F9) has a mini calendar, today's date and clock, and upcoming items.
* **To Do:** every task in one place (the To Do list, daily and weekly to-dos, project tasks), with priority 1–3, start and due dates, categories and notes. Colors show the status: overdue red, current green, upcoming blue, done struck through. Sort by priority, status, start, due date, category or source; the priority tabs on the page edge filter the list.
* **Year Planner:** a colored year grid. Pick a marker (vacation, holiday, meeting, travel…), then drag across days. Click a marked day to edit it. Markers can be renamed and recolored, and marked periods show up in the Day, Week and Month views.
* **Calls:** planned and completed calls with contact, time, direction, topic and notes. The Day page lists that day's calls and has a "Create a call for…" picker.
* **Contact history:** each contact shows calls, linked events and notes, projects, and your own activity notes, newest first.
* **Categories:** shared colored tags on tasks, events, notes, projects and contacts, with category filters in the status bars.
* **Notepad contents:** numbered pages, chapters, page colors, created and modified dates, and Confidential pages that stay hidden until you click Show content.
* **Anniversaries:** birthdays (with age) and your own yearly dates, month by month.
* **Display density:** Comfortable, Compact (default) or Tight, in View → Density or Settings.
* **Themes:** Sand, Nordic, Classic and Marble (a teal marble desk, in the spirit of Organizer 6.1).

To add a language, copy the `en:` block in the `I18N` table near the top of the script in `src/index.html`. Give it a new code such as `it:` and translate the text, then add the code to `LOCALES` and to the language lists.

```
organizer-app/
├── src/index.html              ← the whole user interface (edit this)
├── src-tauri/
│   ├── tauri.conf.json         ← app name, window size, installer settings
│   ├── src/main.rs             ← native entry point
│   ├── Cargo.toml
│   ├── capabilities/           ← what the UI is allowed to call
│   └── icons/                  ← generated from app-icon.png
├── app-icon.png                ← source icon (1024×1024)
└── .github/workflows/build.yml ← builds Windows, macOS and Linux installers on GitHub
```

## 1. One-time setup

You need **Node.js 18 or newer** and **Rust** (install Rust from https://rustup.rs) on every OS. Each OS also needs the following:

| OS | Also install |
|---|---|
| **Windows 10/11** | "Microsoft C++ Build Tools" (in the Visual Studio Installer, tick *Desktop development with C++*). WebView2 is already part of Windows 10/11. |
| **macOS** | Xcode command line tools: `xcode-select --install` |
| **Linux (Debian/Ubuntu)** | `sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev` |

Then, in this folder:

```bash
npm install
```

## 2. Run while developing

```bash
npm run dev
```

This opens the app in a window. The first run compiles Rust and takes a few minutes; later runs take seconds. After you edit `src/index.html`, press **Ctrl+R** (Cmd+R on a Mac) in the window to reload.

## 3. Build an installer

```bash
npm run build
```

The installers end up in `src-tauri/target/release/bundle/`:

| OS | Output |
|---|---|
| Windows | `msi/Organizer_0.1.0_x64_en-US.msi` and `nsis/Organizer_0.1.0_x64-setup.exe` |
| macOS | `dmg/Organizer_0.1.0_aarch64.dmg` and `macos/Organizer.app` |
| Linux | `deb/…deb`, `rpm/…rpm`, `appimage/…AppImage` |

Each OS builds its own installers, so you can't build a Windows `.exe` on a Mac, for example. To get all three from one place, use GitHub (next section).

## 4. Build for all three OSes on GitHub (optional)

1. Push this folder to a GitHub repository.
2. Open **Actions → Build installers → Run workflow**, or push a tag such as `v0.1.0`.
3. After about 15 minutes, a draft **Release** has the `.msi`, `.exe`, `.dmg`, `.deb` and `.AppImage` attached.

## Where your data is stored

Everything is kept in one database file, `organizer.db`, with attached files in an `attachments` folder next to it. On first start, version 0.8 moves your existing entries into this database automatically.

| OS | Folder |
|---|---|
| Windows | `%APPDATA%\com.personal.organizer\` |
| macOS | `~/Library/Application Support/com.personal.organizer/` |
| Linux | `~/.local/share/com.personal.organizer/` |

**Settings → Data storage** chooses how data is kept:

* **Local only, with backups** (available now): the database stays on this computer. Pick a backup folder, ideally inside Google Drive, OneDrive, iCloud Drive or Dropbox, so the copies end up on the web. Backups run automatically (at every start or once a day), keep the newest 5–100 copies, and include attachments. **Restore from backup…** brings back any copy.
* **Server: database and sharing** (coming soon): sync with your own server and share calendars, tasks and projects with other people.

Don't move `organizer.db` itself into a cloud-synced folder; only the backups belong there.

The full schema, including the tables prepared for Google Calendar and iCloud (CalDAV) sync, is described in [docs/DATA_MODEL.md](docs/DATA_MODEL.md).

## Things to change

* **App ID:** `identifier` in `src-tauri/tauri.conf.json` (e.g. `hr.yourname.organizer`). Change it before you start using the app for real, because the data folder is named after it.
* **Name and version:** `productName` and `version` in the same file.
* **Icon:** replace `app-icon.png` and run `npx tauri icon app-icon.png`.

## Notes

* **Unsigned installers:** Windows SmartScreen and macOS Gatekeeper will warn the first time you open the app. Click *More info → Run anyway* on Windows. On a Mac, right-click the app → *Open*. Code signing comes later.
* **Font without internet:** the font (Manrope) loads from Google Fonts. Without internet the app falls back to the system font. Bundling the font into the app is on the to-do list.
