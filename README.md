# glm-overflow

Bar stat pemakaian langganan GLM coding plan — pill overlay kecil yang selalu tampil
di kanan-atas layar (always-on-top, tanpa taskbar) + tray icon. Dibangun dengan
**Tauri 2** (Rust backend, UI vanilla TS/CSS, binary ringan).

## Download

Ambil installer dari [**Releases**](https://github.com/imhcode/glm-prox-monitor/releases/latest):

| Platform | File |
| --- | --- |
| Windows | `glm-overflow_0.2.0_x64-setup.exe` (NSIS, instalasi per-user) |
| Linux (Debian/Ubuntu) | `glm-overflow_0.2.0_amd64.deb` |
| Linux (universal) | `glm-overflow_0.2.0_amd64.AppImage` |

> Installer belum di-code-sign → SmartScreen/antivirus bisa menampilkan peringatan
> pertama kali dijalankan. Data pada screenshot di bawah adalah data dummy.

## Screenshot

**Pill bar** — selalu tampil di kanan-atas layar:

![Pill bar glm-overflow](docs/screenshot-bar.png)

**Panel detail** — klik pill untuk melihat rincian langganan:

![Panel detail glm-overflow](docs/screenshot-panel.png)

## Fitur

- **Pill bar**: progress pemakaian window 5 jam, **% sisa**, **sisa token**, dan countdown reset.
  - hijau (aman) → amber (<20% sisa) → merah (<5% sisa / kena limit)
- **Panel detail** (klik pill): nama, model, sisa token, window mulai/berakhir, total
  request/token, last used, expiry langganan, key — semua waktu ditampilkan **WIB**.
- **Tray icon**: tooltip berisi ringkasan; klik kiri toggle bar; menu Show/Hide · Refresh Now ·
  Settings · Quit.
- **Settings** (di panel): token, base URL, interval refresh (10–3600 detik).
- **State otomatis**: rate-limited (hitung mundur dari `error.window_ends_at`), offline
  (retry tiap interval).
- Konfigurasi tersimpan di:
  - Windows: `%APPDATA%\glm-overflow\config.json`
  - Linux: `~/.config/glm-overflow/config.json`

## Endpoint

`GET {base_url}/stats` dengan header `Authorization: Bearer <token>`.
Default `base_url`: `https://glm.ajianaz.dev`. Field utama yang dipakai:
`token_limit_per_5h`, `current_usage.tokens_used_in_current_window`,
`current_usage.remaining_tokens`, `current_usage.window_ends_at`, `expiry_date`,
`last_used`, `is_expired`, `total_requests`, `total_lifetime_tokens`.

## Development

Prasyarat: Node 18+, Rust (rustup), dan:

- **Windows**: MSVC Build Tools dengan workload C++ (`Microsoft.VisualStudio.Workload.VCTools`)
  + WebView2 runtime (sudah bawaan Win10/11).
- **Linux**: `sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`

```bash
npm install
npm run tauri dev      # mode development (hot reload)
npm run tauri build    # produksi + installer
```

Hasil build:

- Windows: `src-tauri/target/release/bundle/nsis/glm-overflow_0.2.0_x64-setup.exe`
- Linux: `bundle/deb/*.deb` dan `bundle/appimage/*.AppImage`

## Release CI

Push tag `v*` (mis. `git tag v0.2.0 && git push origin v0.2.0`) → GitHub Actions
(`.github/workflows/release.yml`) membangun installer Windows + Linux dan
mem-publish-nya langsung sebagai GitHub Release (lengkap dengan release notes
otomatis).

## Catatan keamanan

- **Token tidak pernah disimpan di source code.** Set token lewat **Settings** di app
  (tersimpan di `config.json` lokal, di luar repo) atau env `GLM_OVERFLOW_TOKEN`
  sebelum first-run. Tanpa token, app tetap jalan dengan state offline.
- Installer belum di-code-sign → SmartScreen/antivirus bisa menampilkan peringatan
  pertama kali dijalankan.
