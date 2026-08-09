'use strict';

/* =====================================================================
   State
   ===================================================================== */

const state = {
  lang: localStorage.getItem('ferrite.lang') || null,
  dict: {},
  job: null,
  projects: [],
  selection: new Map(),   // "projectId|ruleId" -> 'ALL' | Set(rel)
  openProjects: new Set(),
  expanded: new Set(),
  risks: new Set(['safe', 'check', 'data']),
  text: '',
  onlyUnignored: false,
  keepExe: localStorage.getItem('ferrite.keepExe') === '1',
  polling: null,
  busy: false,
};

const KEEP_PATTERNS = ['*.exe'];

const $ = (id) => document.getElementById(id);

/* =====================================================================
   i18n
   ===================================================================== */

function t(key, params) {
  let node = state.dict;
  for (const part of key.split('.')) {
    if (!node || typeof node !== 'object' || !(part in node)) return key;
    node = node[part];
  }
  if (typeof node !== 'string') return key;
  if (params) {
    for (const [name, value] of Object.entries(params)) {
      node = node.split('{' + name + '}').join(String(value));
    }
  }
  return node;
}

function applyI18n() {
  document.documentElement.lang = state.lang;
  document.title = t('app.title') + ' - ' + t('app.tagline');
  document.querySelectorAll('[data-i18n]').forEach((el) => {
    el.textContent = t(el.dataset.i18n);
  });
  document.querySelectorAll('[data-i18n-attr]').forEach((el) => {
    const [attr, key] = el.dataset.i18nAttr.split(':');
    el.setAttribute(attr, t(key));
  });
  $('keep-exe-label').title = t('filter.keep_exe_tip');
}

async function loadLanguages() {
  const data = await (await fetch('/api/languages')).json();
  if (!state.lang) {
    const browser = (navigator.language || data.default).split('-')[0];
    state.lang = data.languages.some((l) => l.code === browser) ? browser : data.default;
  }
  const select = $('lang-select');
  select.innerHTML = data.languages
    .map((l) => `<option value="${l.code}">${escapeHtml(l.label)}</option>`).join('');
  select.value = state.lang;
  select.addEventListener('change', async () => {
    state.lang = select.value;
    localStorage.setItem('ferrite.lang', state.lang);
    await loadDict();
    applyI18n();
    render();
  });
}

async function loadDict() {
  state.dict = await (await fetch('/api/i18n/' + state.lang)).json();
}

/* =====================================================================
   Formatting
   ===================================================================== */

function fmtSize(bytes) {
  const units = [
    [1024 ** 4, 'unit.tb'], [1024 ** 3, 'unit.gb'],
    [1024 ** 2, 'unit.mb'], [1024, 'unit.kb'],
  ];
  for (const [factor, key] of units) {
    if (bytes >= factor) {
      const value = bytes / factor;
      return value.toFixed(value >= 100 ? 0 : value >= 10 ? 1 : 2) + ' ' + t(key);
    }
  }
  return bytes + ' ' + t('unit.bytes');
}

function fmtNum(n) {
  return new Intl.NumberFormat(state.lang === 'fr' ? 'fr-FR' : 'en-US').format(n);
}

function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, (c) => (
    { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]
  ));
}

/* =====================================================================
   Scan
   ===================================================================== */

async function startScan() {
  const workspace = $('workspace').value.trim();
  if (!workspace) { toast(t('error.missing_workspace'), 'err'); return; }
  localStorage.setItem('ferrite.workspace', workspace);

  const res = await fetch('/api/scan', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ workspace, depth: Number($('depth').value), lang: state.lang }),
  });
  const data = await res.json();
  if (!res.ok) { toast(data.error || t('toast.clean_fail'), 'err'); return; }

  state.job = data.id;
  state.projects = [];
  state.selection.clear();
  state.openProjects.clear();
  state.expanded.clear();

  $('scan-btn').disabled = true;
  $('progress').classList.remove('hidden');
  $('summary').classList.add('hidden');
  $('toolbar').classList.add('hidden');
  $('projects').innerHTML = '';
  $('empty-state').classList.add('hidden');
  updateProgress(data);

  state.polling = setInterval(pollScan, 400);
}

async function pollScan() {
  if (!state.job) return;
  const res = await fetch(`/api/scan/${state.job}?lang=${state.lang}`);
  const data = await res.json();
  if (!res.ok) { stopPolling(); toast(data.error, 'err'); return; }

  updateProgress(data);
  if (!['done', 'cancelled', 'error'].includes(data.status)) return;

  stopPolling();
  $('progress').classList.add('hidden');
  $('scan-btn').disabled = false;
  $('scan-btn').textContent = t('form.rescan');

  if (data.status === 'error') { toast(t('toast.scan_error', { message: data.error }), 'err'); return; }
  if (data.status === 'cancelled') toast(t('scan.cancelled'), 'warn');

  state.projects = data.projects || [];
  if (state.projects.length) {
    toast(t('scan.done', { count: state.projects.length, seconds: data.elapsed }), 'ok');
  }
  render();
}

function stopPolling() {
  if (state.polling) { clearInterval(state.polling); state.polling = null; }
}

function updateProgress(data) {
  const pct = data.total ? Math.round((data.done / data.total) * 100) : 0;
  $('progress-fill').style.width = pct + '%';
  $('progress-label').textContent = data.status === 'discovering'
    ? t('scan.discovering')
    : t('scan.progress', { done: data.done, total: data.total });
  $('progress-current').textContent = data.current ? t('scan.scanning', { name: data.current }) : '';
}

async function refreshJob() {
  const res = await fetch(`/api/scan/${state.job}?lang=${state.lang}`);
  const data = await res.json();
  if (res.ok && data.projects) { state.projects = data.projects; render(); }
}

/* =====================================================================
   Filters and selection
   ===================================================================== */

function key(projectId, ruleId) { return projectId + '|' + ruleId; }

function itemVisible(project, item) {
  if (!state.risks.has(item.risk)) return false;
  if (state.onlyUnignored && item.ignore_status === 'all') return false;
  if (state.text) {
    const haystack = [
      project.name, project.path, item.name, item.rule_id,
      t('rules.' + item.rule_id + '.desc'), t('cat.' + item.category),
    ].join(' ').toLowerCase();
    if (!haystack.includes(state.text)) return false;
  }
  return true;
}

function visibleProjects() {
  return state.projects
    .map((project) => ({ project, items: project.items.filter((i) => itemVisible(project, i)) }))
    .filter((entry) => entry.items.length > 0);
}

function selectedRelsFor(item, projectId) {
  const value = state.selection.get(key(projectId, item.rule_id));
  if (value === 'ALL') return null;
  return value || new Set();
}

function isItemFullySelected(projectId, item) {
  const value = state.selection.get(key(projectId, item.rule_id));
  if (value === 'ALL') return true;
  return Boolean(value && value.size === item.count);
}

function isItemPartiallySelected(projectId, item) {
  const value = state.selection.get(key(projectId, item.rule_id));
  return Boolean(value && value !== 'ALL' && value.size > 0 && value.size < item.count);
}

function setItemSelected(projectId, item, on) {
  const k = key(projectId, item.rule_id);
  if (on) state.selection.set(k, 'ALL'); else state.selection.delete(k);
}

function toggleOccurrence(projectId, item, rel, on) {
  const k = key(projectId, item.rule_id);
  let value = state.selection.get(k);
  if (value === 'ALL') value = new Set(item.occurrences.map((o) => o.rel));
  value = value ? new Set(value) : new Set();
  if (on) value.add(rel); else value.delete(rel);

  if (value.size === 0) state.selection.delete(k);
  else if (value.size === item.count) state.selection.set(k, 'ALL');
  else state.selection.set(k, value);
}

function selectionSummary() {
  let count = 0, size = 0, files = 0, tracked = 0, dataSize = 0, dataCount = 0;
  const projects = new Set();

  for (const project of state.projects) {
    for (const item of project.items) {
      const value = state.selection.get(key(project.id, item.rule_id));
      if (!value) continue;
      projects.add(project.id);

      let selSize, selFiles, selCount;
      if (value === 'ALL') {
        selSize = item.size; selFiles = item.files; selCount = item.count;
      } else {
        const chosen = item.occurrences.filter((o) => value.has(o.rel));
        selSize = chosen.reduce((a, o) => a + o.size, 0);
        selFiles = chosen.reduce((a, o) => a + o.files, 0);
        selCount = chosen.length;
      }
      count += selCount; size += selSize; files += selFiles;
      if (item.tracked_count > 0) tracked += item.tracked_count;
      if (item.risk === 'data') { dataSize += selSize; dataCount += selCount; }
    }
  }
  return { count, size, files, tracked, dataSize, dataCount, projects: projects.size };
}

function buildSelections() {
  const out = [];
  for (const project of state.projects) {
    for (const item of project.items) {
      const value = state.selection.get(key(project.id, item.rule_id));
      if (!value) continue;
      out.push({
        project: project.id,
        rule_id: item.rule_id,
        rels: value === 'ALL' ? null : Array.from(value),
      });
    }
  }
  return out;
}

/* =====================================================================
   Rendering
   ===================================================================== */

function render() {
  const entries = visibleProjects();
  const totals = state.projects.reduce((acc, p) => {
    acc.total += p.total_size;
    acc.reclaim += p.reclaimable_size;
    acc.files += p.reclaimable_files;
    acc.git += p.git_size;
    return acc;
  }, { total: 0, reclaim: 0, files: 0, git: 0 });

  if (state.projects.length === 0) {
    $('summary').classList.add('hidden');
    $('toolbar').classList.add('hidden');
    $('projects').innerHTML = '';
    $('empty-state').textContent = t('scan.empty');
    $('empty-state').classList.toggle('hidden', !state.job);
    updateActionBar();
    return;
  }

  $('summary').classList.remove('hidden');
  $('toolbar').classList.remove('hidden');

  const ratio = totals.total ? (totals.reclaim / totals.total) * 100 : 0;
  const donutEl = $('donut');
  if (donutEl) {
    const degrees = (ratio / 100) * 360;
    donutEl.style.background = `conic-gradient(var(--orange) ${degrees}deg, var(--bg-input) ${degrees}deg)`;
  }
  const donutVal = $('donut-value');
  if (donutVal) {
    donutVal.textContent = ratio.toFixed(ratio >= 10 ? 0 : 1) + '%';
  }

  $('stat-projects').textContent = fmtNum(state.projects.length);
  $('stat-total').textContent = fmtSize(totals.total);
  $('stat-reclaim').textContent = fmtSize(totals.reclaim);
  $('stat-files').textContent = fmtNum(totals.files);
  $('stat-git').textContent = fmtSize(totals.git);

  if (entries.length === 0) {
    $('projects').innerHTML = '';
    $('empty-state').textContent = t('scan.no_items');
    $('empty-state').classList.remove('hidden');
  } else {
    $('empty-state').classList.add('hidden');
    $('projects').innerHTML = entries
      .sort((a, b) => b.project.reclaimable_size - a.project.reclaimable_size)
      .map(renderProject).join('');
  }

  updateActionBar();
}

function countIgnoreStatuses(items) {
  const counts = { all: 0, partial: 0, none: 0, tracked: 0, na: 0 };
  items.forEach((item) => { counts[item.ignore_status] = (counts[item.ignore_status] || 0) + 1; });
  return counts;
}

function needsIgnoreFix(counts) {
  return counts.none + counts.partial + counts.tracked;
}

/* .gitignore coverage summary, readable without expanding the project. */
function renderIgnoreSummary(project, items) {
  if (!project.is_git) return '';
  const counts = countIgnoreStatuses(items);
  const badges = [];

  const push = (status) => {
    if (!counts[status]) return;
    badges.push(`<span class="badge ign-${status}" title="${escapeHtml(t('ignore.tip_' + status))}">`
      + `${t('ignore.' + status)} ${counts[status]}</span>`);
  };

  push('none');
  push('partial');
  push('tracked');
  if (needsIgnoreFix(counts) === 0 && counts.all > 0) push('all');

  if (needsIgnoreFix(counts) > 0) {
    badges.push(`<button class="fix-btn" data-action="fix-gitignore"`
      + ` title="${escapeHtml(t('ignore.fix_tip'))}">${t('ignore.add_short')}</button>`);
  }
  return badges.join('');
}

function renderProject({ project, items }) {
  const open = state.openProjects.has(project.id);
  const visibleSize = items.reduce((a, i) => a + i.size, 0);
  const ratio = project.total_size ? (visibleSize / project.total_size) * 100 : 0;
  const allSelected = items.every((i) => isItemFullySelected(project.id, i));
  const someSelected = items.some((i) =>
    isItemFullySelected(project.id, i) || isItemPartiallySelected(project.id, i));
  const maxItem = Math.max(...items.map((i) => i.size), 1);

  const gitBadge = project.is_git
    ? `<span class="badge git">git${project.branch ? ' / ' + escapeHtml(project.branch) : ''}</span>`
    : `<span class="badge nogit">${t('repo.not_git')}</span>`;

  return `
<div class="project ${open ? 'open' : ''}" data-project="${project.id}">
  <div class="project-head" data-action="toggle-project">
    <div class="project-check">
      <input type="checkbox" data-action="select-project"
             ${allSelected ? 'checked' : ''} ${!allSelected && someSelected ? 'data-indeterminate="1"' : ''}>
    </div>
    <div class="project-main">
      <div class="project-title">
        <span class="caret">&#9654;</span>
        <span class="name">${escapeHtml(project.name)}</span>
        ${gitBadge}
        ${renderIgnoreSummary(project, items)}
        <span class="path">${escapeHtml(project.path)}</span>
      </div>
      <div class="project-bar-wrap">
        <div class="bar"><div class="bar-fill reclaim" style="width:${ratio.toFixed(1)}%"></div></div>
        <span class="bar-meta">${ratio.toFixed(1)}% ${t('repo.of_total', { total: fmtSize(project.total_size) })}</span>
      </div>
    </div>
    <div class="project-figures">
      <span class="big">${fmtSize(visibleSize)}</span>
      <span class="sub">${fmtNum(items.reduce((a, i) => a + i.files, 0))} ${t('unit.files')}</span>
      <span class="sub">${t('repo.git_size')} ${fmtSize(project.git_size)}</span>
    </div>
  </div>
  ${open ? `
  <div class="project-body">
    <div class="project-tools">
      <button class="btn ghost small" data-action="select-safe-project">${t('repo.select_safe')}</button>
      <button class="btn ghost small" data-action="select-all-project">${t('repo.select_all')}</button>
      ${project.is_git ? `<button class="btn ghost small" data-action="gitgc">${t('repo.gc')}</button>` : ''}
    </div>
    ${items.map((item) => renderItem(project, item, maxItem)).join('')}
  </div>` : ''}
</div>`;
}

function renderItem(project, item, maxItem) {
  const k = key(project.id, item.rule_id);
  const expanded = state.expanded.has(k);
  const full = isItemFullySelected(project.id, item);
  const partial = isItemPartiallySelected(project.id, item);
  const width = Math.max(2, (item.size / maxItem) * 100);

  const ignoreClass = project.is_git ? 'ign-' + item.ignore_status : 'ign-na';
  const ignoreLabel = project.is_git ? t('ignore.' + item.ignore_status) : t('ignore.na');
  const ignoreTip = project.is_git ? t('ignore.tip_' + item.ignore_status) : t('ignore.tip_na');

  const trackedBadge = item.tracked_count > 0
    ? `<span class="badge tracked" title="${escapeHtml(t('tracked.tip', { count: item.tracked_count }))}">${t('tracked.badge')}</span>`
    : '';

  return `
<div class="item" data-rule="${escapeHtml(item.rule_id)}">
  <input type="checkbox" data-action="select-item" ${full ? 'checked' : ''}
         ${partial ? 'data-indeterminate="1"' : ''}>
  <div class="item-main">
    <div class="item-name-row">
      <span class="item-name">${escapeHtml(item.name)}</span>
      <span class="badge ${item.risk}" title="${escapeHtml(t('risk.' + item.risk + '_desc'))}">${t('risk.' + item.risk)}</span>
      <span class="badge cat">${t('cat.' + item.category)}</span>
      <span class="badge ${ignoreClass}" title="${escapeHtml(ignoreTip)}">${ignoreLabel}</span>
      ${trackedBadge}
    </div>
    <div class="item-desc">${escapeHtml(t('rules.' + item.rule_id + '.desc'))}</div>
    <div class="item-restore">${t('item.restore')}: <b>${escapeHtml(item.restore)}</b></div>
  </div>
  <div class="item-gauge">
    <div class="bar"><div class="bar-fill item ${item.risk}" style="width:${width.toFixed(1)}%"></div></div>
    <span class="bar-meta">${item.count === 1 ? t('item.occurrence_one') : t('item.occurrences', { count: item.count })}</span>
  </div>
  <div class="item-figures">
    <div class="size">${fmtSize(item.size)}</div>
    <div class="files">${fmtNum(item.files)} ${t('unit.files')}</div>
  </div>
  <button class="expand-btn" data-action="expand">${expanded ? t('item.collapse') : t('item.expand')}</button>
  ${expanded ? renderOccurrences(project, item, selectedRelsFor(item, project.id)) : ''}
</div>`;
}

function renderOccurrences(project, item, selected) {
  const locked = item.truncated > 0;
  const rows = item.occurrences.map((occ) => {
    const checked = selected === null || (selected && selected.has(occ.rel));
    let flag = 'na';
    let title = t('ignore.tip_na');
    if (project.is_git) {
      if (occ.ignored) { flag = 'ok'; title = t('ignore.tip_all'); }
      else if (occ.tracked) { flag = 'trk'; title = t('ignore.tip_tracked'); }
      else { flag = 'no'; title = t('ignore.tip_none'); }
    }
    return `
    <div class="occ">
      <input type="checkbox" data-action="select-occ" data-rel="${escapeHtml(occ.rel)}"
             ${checked ? 'checked' : ''} ${locked ? 'disabled' : ''}>
      <span class="rel" title="${escapeHtml(occ.rel)}">${escapeHtml(occ.rel)}${occ.tracked ? ' *' : ''}</span>
      <span class="osize">${fmtSize(occ.size)}</span>
      <span class="ofiles">${fmtNum(occ.files)}</span>
      <span class="flag ${flag}" title="${escapeHtml(title)}"></span>
    </div>`;
  }).join('');

  const more = item.truncated > 0
    ? `<div class="occ-more">${t('item.truncated', { count: item.truncated })}</div>` : '';

  return `<div class="occurrences">${rows}${more}</div>`;
}

function syncIndeterminate() {
  document.querySelectorAll('input[data-indeterminate]').forEach((el) => { el.indeterminate = true; });
}

function updateActionBar() {
  const summary = selectionSummary();
  const bar = $('actionbar');
  $('stat-selected').textContent = summary.count ? fmtSize(summary.size) : '0';

  if (summary.count === 0) { bar.classList.add('hidden'); return; }
  bar.classList.remove('hidden');

  $('action-count').textContent = t('select.summary', {
    count: fmtNum(summary.count), size: fmtNum(summary.files) + ' ' + t('unit.files'),
  });
  $('action-size').textContent = fmtSize(summary.size);

  const keepPill = $('action-keep');
  keepPill.classList.toggle('hidden', !state.keepExe);
  keepPill.textContent = t('filter.keep_exe');

  const trackedPill = $('action-tracked');
  trackedPill.classList.toggle('hidden', summary.tracked === 0);
  if (summary.tracked) trackedPill.textContent = t('tracked.badge') + ': ' + fmtNum(summary.tracked);

  const dataPill = $('action-data');
  dataPill.classList.toggle('hidden', summary.dataCount === 0);
  if (summary.dataCount) dataPill.textContent = t('risk.data') + ': ' + fmtSize(summary.dataSize);
}

function redraw() { render(); syncIndeterminate(); }

/* =====================================================================
   Interactions
   ===================================================================== */

const findProject = (id) => state.projects.find((p) => p.id === Number(id));
const findItem = (project, ruleId) => project.items.find((i) => i.rule_id === ruleId);

$('projects').addEventListener('click', (event) => {
  const projectEl = event.target.closest('.project');
  if (!projectEl) return;
  const project = findProject(projectEl.dataset.project);
  if (!project) return;

  const itemEl = event.target.closest('.item');
  const item = itemEl ? findItem(project, itemEl.dataset.rule) : null;
  const action = event.target.closest('[data-action]')?.dataset.action;

  if (action === 'expand' && item) {
    const k = key(project.id, item.rule_id);
    state.expanded.has(k) ? state.expanded.delete(k) : state.expanded.add(k);
    redraw();
    return;
  }
  if (action === 'select-safe-project') {
    project.items.filter((i) => i.risk === 'safe' && itemVisible(project, i))
      .forEach((i) => setItemSelected(project.id, i, true));
    redraw(); return;
  }
  if (action === 'select-all-project') {
    project.items.filter((i) => itemVisible(project, i))
      .forEach((i) => setItemSelected(project.id, i, true));
    redraw(); return;
  }
  if (action === 'gitgc') { runGitGc(project); return; }
  if (action === 'fix-gitignore') {
    const targets = project.items.filter((i) => itemVisible(project, i) && i.ignore_status !== 'all');
    sendGitignore(targets.map((i) => ({ project: project.id, rule_id: i.rule_id, rels: null })));
    return;
  }

  if (action === 'toggle-project' || event.target.closest('.project-head')) {
    if (event.target.matches('input[type=checkbox]')) return;
    state.openProjects.has(project.id)
      ? state.openProjects.delete(project.id)
      : state.openProjects.add(project.id);
    redraw();
  }
});

$('projects').addEventListener('change', (event) => {
  const input = event.target;
  if (input.type !== 'checkbox') return;
  const project = findProject(input.closest('.project').dataset.project);
  if (!project) return;

  const action = input.dataset.action;
  if (action === 'select-project') {
    project.items.filter((i) => itemVisible(project, i))
      .forEach((i) => setItemSelected(project.id, i, input.checked));
  } else if (action === 'select-item') {
    setItemSelected(project.id, findItem(project, input.closest('.item').dataset.rule), input.checked);
  } else if (action === 'select-occ') {
    const item = findItem(project, input.closest('.item').dataset.rule);
    toggleOccurrence(project.id, item, input.dataset.rel, input.checked);
  } else return;

  redraw();
});

$('filter-text').addEventListener('input', (e) => {
  state.text = e.target.value.trim().toLowerCase();
  redraw();
});

$('only-unignored').addEventListener('change', (e) => {
  state.onlyUnignored = e.target.checked;
  redraw();
});

$('keep-exe').addEventListener('change', (e) => {
  state.keepExe = e.target.checked;
  localStorage.setItem('ferrite.keepExe', state.keepExe ? '1' : '0');
  $('keep-exe-label').classList.toggle('on', state.keepExe);
  updateActionBar();
});

$('risk-chips').addEventListener('click', (event) => {
  const chip = event.target.closest('.chip');
  if (!chip) return;
  const risk = chip.dataset.risk;
  state.risks.has(risk) ? state.risks.delete(risk) : state.risks.add(risk);
  chip.classList.toggle('active', state.risks.has(risk));
  redraw();
});

$('sel-all').addEventListener('click', () => {
  visibleProjects().forEach(({ project, items }) =>
    items.forEach((i) => setItemSelected(project.id, i, true)));
  redraw();
});

$('sel-safe').addEventListener('click', () => {
  visibleProjects().forEach(({ project, items }) =>
    items.filter((i) => i.risk === 'safe').forEach((i) => setItemSelected(project.id, i, true)));
  redraw();
});

$('sel-none').addEventListener('click', () => { state.selection.clear(); redraw(); });

$('scan-btn').addEventListener('click', startScan);
$('workspace').addEventListener('keydown', (e) => { if (e.key === 'Enter') startScan(); });
// Browse removed: native OS dialog via PowerShell would flash a terminal
// window. Users type or paste the path directly.
$('cancel-btn').addEventListener('click', async () => {
  if (state.job) await fetch(`/api/scan/${state.job}/cancel`, { method: 'POST' });
});

/* =====================================================================
   Actions
   ===================================================================== */

$('btn-clean').addEventListener('click', () => {
  const summary = selectionSummary();
  if (!summary.count) { toast(t('toast.nothing_selected'), 'warn'); return; }

  $('modal-body').textContent = t('action.confirm_body', {
    count: fmtNum(summary.count), size: fmtSize(summary.size), projects: summary.projects,
  });

  const warnings = [];
  if (state.keepExe) {
    warnings.push(`<div class="modal-warning keep">${escapeHtml(t('action.keep_exe_note'))}</div>`);
  }
  if (summary.tracked > 0) {
    warnings.push(`<div class="modal-warning">${escapeHtml(t('action.warn_tracked', { count: summary.tracked }))}</div>`);
  }
  if (summary.dataCount > 0) {
    warnings.push(`<div class="modal-warning data">${escapeHtml(t('action.warn_data', { count: summary.dataCount, size: fmtSize(summary.dataSize) }))}</div>`);
  }
  $('modal-warnings').innerHTML = warnings.join('');
  $('modal').classList.remove('hidden');
});

$('modal-cancel').addEventListener('click', () => $('modal').classList.add('hidden'));

$('modal-confirm').addEventListener('click', async () => {
  $('modal').classList.add('hidden');
  if (state.busy) return;
  state.busy = true;
  $('btn-clean').disabled = true;
  $('btn-clean').textContent = t('action.cleaning');

  try {
    const res = await fetch('/api/clean', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        job: state.job,
        selections: buildSelections(),
        keep: state.keepExe ? KEEP_PATTERNS : [],
        lang: state.lang,
      }),
    });
    const data = await res.json();
    if (!res.ok) { toast(data.error || t('toast.clean_fail'), 'err'); return; }

    if (data.failed === 0) {
      toast(t('toast.clean_ok', { count: fmtNum(data.ok), size: fmtSize(data.freed) }), 'ok');
    } else {
      toast(t('toast.clean_partial', {
        ok: fmtNum(data.ok), size: fmtSize(data.freed), failed: fmtNum(data.failed),
      }), 'warn');
      (data.failures || []).slice(0, 3).forEach((f) => toast(`${f.rel}: ${f.error}`, 'err'));
    }
    if (data.kept_files > 0) {
      toast(t('toast.clean_kept', {
        count: fmtNum(data.kept_files), size: fmtSize(data.kept_size),
      }), 'info');
    }
    state.selection.clear();
    await refreshJob();
  } finally {
    state.busy = false;
    $('btn-clean').disabled = false;
    $('btn-clean').textContent = t('action.clean');
  }
});

async function sendGitignore(selections) {
  if (!selections.length) { toast(t('toast.nothing_selected'), 'warn'); return; }

  const res = await fetch('/api/gitignore', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ job: state.job, selections, lang: state.lang }),
  });
  const data = await res.json();
  if (!res.ok) { toast(data.error, 'err'); return; }

  if (data.added > 0) toast(t('toast.gitignore_ok', { count: data.added, repos: data.repos }), 'ok');
  else toast(t('toast.gitignore_none'), 'info');
  if (data.skipped > 0) toast(t('toast.gitignore_skipped', { count: data.skipped }), 'warn');

  // A pattern only takes effect once the path has left the git index.
  const stillTracked = (data.updates || [])
    .flatMap((update) => update.statuses || [])
    .filter((status) => status.ignore_status === 'tracked').length;
  if (stillTracked > 0) toast(t('toast.gitignore_tracked', { count: stillTracked }), 'warn');

  await refreshJob();
}

$('btn-gitignore').addEventListener('click', () => sendGitignore(buildSelections()));

async function runGitGc(project) {
  const res = await fetch('/api/gitgc', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ job: state.job, project: project.id, lang: state.lang }),
  });
  const data = await res.json();
  if (!res.ok || !data.ok) { toast(data.error || t('toast.gitgc_fail'), 'err'); return; }
  if (data.freed > 0) toast(t('toast.gitgc_ok', { size: fmtSize(data.freed) }), 'ok');
  else toast(t('toast.gitgc_none'), 'info');
  await refreshJob();
}

/* =====================================================================
   Toasts
   ===================================================================== */

function toast(message, kind = 'info') {
  const el = document.createElement('div');
  el.className = 'toast ' + kind;
  el.textContent = message;
  $('toasts').appendChild(el);
  setTimeout(() => el.remove(), 6000);
}

/* =====================================================================
   Bootstrap
   ===================================================================== */

(async function boot() {
  await loadLanguages();
  await loadDict();
  applyI18n();

  $('keep-exe').checked = state.keepExe;
  $('keep-exe-label').classList.toggle('on', state.keepExe);

  const saved = localStorage.getItem('ferrite.workspace');
  if (saved) $('workspace').value = saved;
  $('workspace').focus();

  const btnBrowse = $('btn-browse');
  const folderPicker = $('folder-picker');

  if (btnBrowse) {
    btnBrowse.addEventListener('click', () => {
      fetch('/api/browse-folder')
        .then(r => r.json())
        .then(data => {
          if (data && data.path) {
            $('workspace').value = data.path;
            localStorage.setItem('ferrite.workspace', data.path);
          } else if (folderPicker) {
            folderPicker.click();
          }
        })
        .catch(() => {
          if (folderPicker) folderPicker.click();
        });
    });
  }

  if (folderPicker) {
    folderPicker.addEventListener('change', (e) => {
      if (e.target.files && e.target.files.length > 0) {
        const file = e.target.files[0];
        const path = file.path || (file.webkitRelativePath ? file.webkitRelativePath.split('/')[0] : '');
        if (path) {
          $('workspace').value = path;
          localStorage.setItem('ferrite.workspace', path);
        }
      }
    });
  }
  document.addEventListener('contextmenu', e => e.preventDefault());
})();
