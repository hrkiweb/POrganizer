# Organizer – desktop app

A personal organizer in a ring-binder style. It's built with [Tauri 2](https://tauri.app): a small native shell (Rust) around the HTML/CSS/JS user interface in `src/index.html`.

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

The app keeps your data in the app's own storage on your computer. Uninstalling the app can remove it, so use **Backup → Copy** now and then. A proper database file (SQLite) and web sync are the next steps in the plan.

| OS | Folder |
|---|---|
| Windows | `%LOCALAPPDATA%\com.personal.organizer\` |
| macOS | `~/Library/WebKit/com.personal.organizer/` |
| Linux | `~/.local/share/com.personal.organizer/` |

## Things to change

* **App ID:** `identifier` in `src-tauri/tauri.conf.json` (e.g. `hr.yourname.organizer`). Change it before you start using the app for real, because the data folder is named after it.
* **Name and version:** `productName` and `version` in the same file.
* **Icon:** replace `app-icon.png` and run `npx tauri icon app-icon.png`.

## Notes

* **Unsigned installers:** Windows SmartScreen and macOS Gatekeeper will warn the first time you open the app. Click *More info → Run anyway* on Windows. On a Mac, right-click the app → *Open*. Code signing comes later.
* **Font without internet:** the font (Manrope) loads from Google Fonts. Without internet the app falls back to the system font. Bundling the font into the app is on the to-do list.
