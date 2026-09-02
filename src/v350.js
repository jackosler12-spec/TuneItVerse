// v3.5.1 — expose invokeCmd so workspace export / heatmap can call Tauri.
(function () {
  function expose() {
    if (typeof invokeCmd === 'function') window.invokeCmd = invokeCmd;
    try {
      if (typeof currentBin !== 'undefined') window.currentBin = currentBin;
    } catch (e) { /* lexical let in another script is invisible */ }
  }
  expose();
  window.addEventListener('DOMContentLoaded', expose);
  setInterval(expose, 800);
})();
