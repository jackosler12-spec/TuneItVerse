// TuneItVerse v3.37.0 — log replay, trim preview, BIN diff report.
// Preview only. Does not enable write.
(function () {
  function parseMaybe(raw) {
    if (raw == null) return null;
    if (typeof raw === 'string') {
      try { return JSON.parse(raw); } catch (_) { return raw; }
    }
    return raw;
  }
  async function cmd(name, args) {
    if (typeof window.invokeCmd !== 'function') throw new Error('backend unavailable');
    return window.invokeCmd(name, args || {});
  }
  function show(id, text) {
    const node = document.getElementById(id);
    if (!node) return;
    node.textContent = text == null ? '' : String(text);
  }
  let timer = null;
  let index = 0;

  async function summary() {
    const raw = parseMaybe(await cmd('log_session_summary'));
    show('log-replay-readout', JSON.stringify(raw, null, 2));
  }
  async function frame(i) {
    const raw = parseMaybe(await cmd('log_replay_frame', { index: i }));
    index = raw && typeof raw.index === 'number' ? raw.index : i;
    const values = raw && raw.values ? raw.values : {};
    const bits = Object.keys(values).slice(0, 8).map(function (k) {
      return k + ' ' + values[k];
    }).join('  ·  ');
    show('log-replay-readout', 'frame ' + (index + 1) + '/' + (raw.count || '?') + '  ' + bits);
    const slider = document.getElementById('log-replay-index');
    if (slider && raw && raw.count) {
      slider.max = String(Math.max(0, raw.count - 1));
      slider.value = String(index);
    }
  }
  function stop() {
    if (timer) clearInterval(timer);
    timer = null;
  }
  async function play() {
    stop();
    timer = setInterval(function () {
      frame(index + 1).catch(function (e) { stop(); show('log-replay-readout', String(e)); });
    }, 200);
    await frame(index);
  }
  async function suggestion() {
    const raw = parseMaybe(await cmd('log_trim_suggestion'));
    show('log-replay-readout', JSON.stringify(raw, null, 2));
    if (raw && raw.write_allowed) show('log-replay-readout', 'Refusing suggestion: write_allowed must stay false.');
  }
  async function diff() {
    const a = window.currentBin ? Array.from(window.currentBin) : null;
    const b = window.__compareBin ? Array.from(window.__compareBin) : null;
    if (!a || !b) {
      show('compare-report', 'Load the working BIN on Maps, then load a compare BIN here.');
      return;
    }
    const text = await cmd('bin_diff_report', { a: a, b: b });
    show('compare-report', text);
  }
  async function loadCompare() {
    const opened = parseMaybe(await cmd('dialog_open_file', { kind: 'bin' }));
    if (!opened || opened.cancelled) return;
    window.__compareBin = opened.bytes || [];
    show('compare-report', 'Compare image loaded (' + window.__compareBin.length + ' bytes). Diff does not write.');
  }

  function boot() {
    document.body.addEventListener('click', function (ev) {
      const btn = ev.target && ev.target.closest ? ev.target.closest('button') : null;
      if (!btn) return;
      const run = {
        'btn-log-summary': summary,
        'btn-log-replay': play,
        'btn-log-replay-stop': stop,
        'btn-log-suggest': suggestion,
        'btn-compare-load': loadCompare,
        'btn-compare-run': diff
      }[btn.id];
      if (!run) return;
      ev.preventDefault();
      Promise.resolve(run()).catch(function (e) {
        show(btn.id.indexOf('compare') === 0 ? 'compare-report' : 'log-replay-readout', String(e));
      });
    });
    const slider = document.getElementById('log-replay-index');
    if (slider) {
      slider.addEventListener('input', function () {
        stop();
        frame(Number(slider.value) || 0).catch(function (e) { show('log-replay-readout', String(e)); });
      });
    }
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else boot();
})();
