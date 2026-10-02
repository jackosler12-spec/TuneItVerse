// TuneItVerse v3.36.0 — project page: fingerprint, log stats, tune project, BIN search.
(function () {
  function parseMaybe(raw) {
    if (raw == null) return null;
    if (typeof raw === "string") {
      try { return JSON.parse(raw); } catch (_) { return raw; }
    }
    return raw;
  }
  async function cmd(name, args) {
    if (typeof window.invokeCmd !== "function") throw new Error("backend unavailable");
    return window.invokeCmd(name, args || {});
  }
  function show(obj) {
    const pre = document.getElementById("project-output");
    if (!pre) return;
    pre.textContent = typeof obj === "string" ? obj : JSON.stringify(obj, null, 2);
  }
  async function openBin() {
    const opened = parseMaybe(await cmd("dialog_open_file", { kind: "bin" }));
    if (!opened || opened.cancelled || !opened.bytes) return null;
    return opened.bytes;
  }
  async function score() {
    const bytes = await openBin();
    if (!bytes) return;
    show(parseMaybe(await cmd("score_fingerprint_cmd", { data: bytes })));
  }
  async function logStats() {
    const opened = parseMaybe(await cmd("dialog_open_file", { kind: "csv" }));
    const csv = opened && !opened.cancelled ? opened.text : null;
    show(parseMaybe(await cmd("analyze_log_channels_cmd", { csv: csv })));
  }
  async function buildProject() {
    const bytes = await openBin();
    if (!bytes) return;
    const notes = (document.getElementById("project-notes") || {}).value || "";
    const project = parseMaybe(await cmd("build_tune_project_cmd", { data: bytes, notes: notes }));
    show(project);
    if (project && project.write_allowed === true) {
      show("Project reported write_allowed. Confirm identify on the Maps page before any flash. This page does not flash.");
    }
    await cmd("dialog_save_text", { text: JSON.stringify(project, null, 2), default_name: "tune-project.json" });
  }
  async function loadProject() {
    const opened = parseMaybe(await cmd("dialog_open_file", { kind: "json" }));
    if (!opened || opened.cancelled) return;
    const loaded = parseMaybe(await cmd("load_tune_project_cmd", { json_text: opened.text || "" }));
    if (loaded && loaded.write_allowed) {
      show("Refusing project: write_allowed must stay false.");
      return;
    }
    show(loaded);
    if (loaded && loaded.notes && document.getElementById("project-notes")) {
      document.getElementById("project-notes").value = loaded.notes;
    }
  }
  async function searchBin() {
    const bytes = await openBin();
    if (!bytes) return;
    const query = (document.getElementById("bin-search-q") || {}).value || "";
    show(parseMaybe(await cmd("search_bin_cmd", { data: bytes, query: query })));
  }
  function boot() {
    document.body.addEventListener("click", function (ev) {
      const btn = ev.target && ev.target.closest ? ev.target.closest("button") : null;
      if (!btn) return;
      const run = {
        "btn-score-fingerprint": score,
        "btn-log-stats": logStats,
        "btn-build-project": buildProject,
        "btn-load-project": loadProject,
        "btn-search-bin": searchBin
      }[btn.id];
      if (!run) return;
      ev.preventDefault();
      Promise.resolve(run()).catch(function (e) { show(String(e)); });
    }, true);
  }
  if (document.readyState === "loading") document.addEventListener("DOMContentLoaded", boot);
  else boot();
})();
