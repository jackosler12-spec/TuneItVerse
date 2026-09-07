# Agent notes

After every code update that ships, build a Windows release binary and copy it to the repository root as `TuneItVerse.exe` (the TuneItVerse folder). Use `scripts/copy-installer.js` (`npm run copy:exe` after `cargo build --release` in `src-tauri`, or `npm run build`). Do not leave the user on a stale root exe.
