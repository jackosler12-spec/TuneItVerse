// TuneItVerse v3.33.0 DOM inject — fills holes left in index.html.
(function () {
  function ensureDomFixes() {
    const title = document.querySelector('title');
    if (title && /v3\.17/.test(title.textContent || '')) title.textContent = 'TuneItVerse v3.33.0';
    document.querySelectorAll('.sidebar-logo-text .version, #status-left').forEach(function (el) {
      if (el && /3\.17/.test(el.textContent || '')) el.textContent = el.textContent.replace(/3\.17\.0/g, '3.33.0');
    });
    const headRow = document.querySelector('#ecu-catalog thead tr');
    if (headRow && headRow.children.length === 6) {
      const th = document.createElement('th');
      th.textContent = 'Write';
      headRow.appendChild(th);
    }
    const loading = document.querySelector('#catalog-tbody tr td[colspan]');
    if (loading) loading.setAttribute('colspan', '7');
    if (!document.getElementById('adapter-list')) {
      const log = document.getElementById('connect-log');
      if (log && log.parentNode) {
        const h = document.createElement('div');
        h.className = 'panel-header';
        h.innerHTML = '<span class="panel-title">Official adapters</span>';
        const pre = document.createElement('pre');
        pre.id = 'adapter-list';
        pre.className = 'mono-block';
        pre.textContent = 'Loading adapter catalog\u2026';
        log.parentNode.insertBefore(h, log.nextSibling);
        log.parentNode.insertBefore(pre, h.nextSibling);
      }
    }
    const level = document.getElementById('seed-level');
    if (level && !document.getElementById('seed-algo')) {
      const algo = document.createElement('input');
      algo.id = 'seed-algo';
      algo.placeholder = 'GM algo 0-1023 (optional)';
      algo.setAttribute('inputmode', 'numeric');
      algo.style.width = '11rem';
      level.parentNode.insertBefore(algo, level.nextSibling);
    }
    if (!document.getElementById('btn-p01-compare')) {
      const keyBtn = document.getElementById('btn-compute-key');
      if (keyBtn && keyBtn.parentNode) {
        const b = document.createElement('button');
        b.id = 'btn-p01-compare';
        b.className = 'btn btn-sm';
        b.type = 'button';
        b.textContent = 'P01 vs table';
        keyBtn.parentNode.insertBefore(b, keyBtn.nextSibling);
      }
    }
  }

  function parseAlgo(raw) {
    const s = (raw || '').trim();
    if (!s) return null;
    const n = s.toLowerCase().startsWith('0x') ? parseInt(s, 16) : parseInt(s, 10);
    if (!Number.isFinite(n) || n < 0 || n > 1023) return undefined;
    return n >>> 0;
  }

  async function invoke(name, args) {
    if (typeof window.invokeCmd === 'function') return window.invokeCmd(name, args || {});
    throw new Error('backend unavailable');
  }

  async function computeSeedWithAlgo() {
    const out = document.getElementById('seed-result');
    const parsed = parseAlgo(document.getElementById('seed-algo') ? document.getElementById('seed-algo').value : '');
    if (parsed === undefined) {
      if (out) out.textContent = 'GM algo must be 0-1023 (or 0x000-0x3FF).';
      return;
    }
    const args = {
      seed_hex: document.getElementById('seed-hex') ? document.getElementById('seed-hex').value : '',
      family: document.getElementById('seed-family') ? document.getElementById('seed-family').value : 'P01_0411',
      level: document.getElementById('seed-level') ? document.getElementById('seed-level').value : '1'
    };
    if (parsed !== null) args.algo = parsed;
    try {
      const raw = await invoke('compute_seed_key', args);
      let obj = raw;
      if (typeof raw === 'string') { try { obj = JSON.parse(raw); } catch (_) {} }
      if (out) out.textContent = typeof obj === 'string' ? obj : JSON.stringify(obj, null, 2);
    } catch (e) {
      if (out) out.textContent = 'Error: ' + e;
    }
  }

  async function compareP01WithAlgo() {
    const out = document.getElementById('seed-result');
    const rawAlgo = document.getElementById('seed-algo') ? document.getElementById('seed-algo').value.trim() : '';
    const scan = rawAlgo === '' || /^scan$/i.test(rawAlgo);
    const args = {
      seed_hex: document.getElementById('seed-hex') ? document.getElementById('seed-hex').value : '',
      scan: scan
    };
    if (!scan) {
      const parsed = parseAlgo(rawAlgo);
      if (parsed === undefined || parsed === null) {
        if (out) out.textContent = 'GM algo must be 0-1023, or leave blank to scan.';
        return;
      }
      args.algo = parsed;
    }
    try {
      const raw = await invoke('compare_p01_gm_seed', args);
      let obj = raw;
      if (typeof raw === 'string') { try { obj = JSON.parse(raw); } catch (_) {} }
      if (out) out.textContent = typeof obj === 'string' ? obj : JSON.stringify(obj, null, 2);
    } catch (e) {
      if (out) out.textContent = 'Error: ' + e;
    }
  }

  function interceptSeedClicks() {
    document.body.addEventListener('click', function (ev) {
      const btn = ev.target && ev.target.closest ? ev.target.closest('button') : null;
      if (!btn) return;
      if (btn.id === 'btn-compute-key') {
        ev.preventDefault();
        ev.stopPropagation();
        Promise.resolve(computeSeedWithAlgo()).catch(function (e) { console.error(e); });
      } else if (btn.id === 'btn-p01-compare') {
        ev.preventDefault();
        ev.stopPropagation();
        Promise.resolve(compareP01WithAlgo()).catch(function (e) { console.error(e); });
      }
    }, true);
  }

  function boot() {
    ensureDomFixes();
    interceptSeedClicks();
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', boot);
  else boot();
})();
