// v3.5.0 — expose invokeCmd so v340 workspace export / heatmap can call Tauri.
(function () {
  function expose() {
    if (typeof invokeCmd === 'function') window.invokeCmd = invokeCmd;
    // var currentBin in main.js becomes window.currentBin; let does not.
    try {
      if (typeof currentBin !== 'undefined') window.currentBin = currentBin;
    } catch (e) { /* lexical let in another script is invisible */ }
  }
  expose();
  window.addEventListener('DOMContentLoaded', expose);
  setInterval(expose, 1500);
})();
