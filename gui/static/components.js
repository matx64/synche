import { requestApi, showApiError } from './api_feedback.js';

function escapeHtml(s) {
  return String(s).replace(
    /[&<>"']/g,
    (c) =>
      ({
        "&": "&amp;",
        "<": "&lt;",
        ">": "&gt;",
        '"': "&quot;",
        "'": "&#39;",
      })[c],
  );
}

export function dirListItem(name) {
  const safe = escapeHtml(name);
  return `<details class="list-item" id="dir-${safe}">
            <summary>
              <strong>
                        <svg class="lucide lucide-folder-open-icon lucide-folder-open" fill="none" height="20"
                             stroke="currentColor" stroke-linecap="round"
                             stroke-linejoin="round" stroke-width="2" viewBox="0 0 24 24" width="20"
                             xmlns="http://www.w3.org/2000/svg">
                            <path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2"/>
                        </svg>
                        <span>${safe}</span>
                    </strong>
            </summary>

            <div class="dir-activity" hidden></div>

            <div class="dir-actions">
            <button class="btn icon-btn remove-dir-btn" type="button">
                <svg
                    xmlns="http://www.w3.org/2000/svg"
                    width="18"
                    height="18"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    stroke-width="2"
                    stroke-linecap="round"
                    stroke-linejoin="round"
                    class="lucide lucide-folder-x-icon lucide-folder-x"
                >
                    <path
                        d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"
                    />
                    <path d="m9.5 10.5 5 5" />
                    <path d="m14.5 10.5-5 5" />
                </svg>
                <span>Stop Syncing</span>
            </button>
            </div>
          </details>`;
}

export function peerListItem({ id, addr, hostname, instance_id, last_seen, sync_dirs }) {
  const tsLabel = last_seen
    ? new Date(last_seen * 1000).toLocaleString()
    : "unknown";
  const dirsList =
    sync_dirs && sync_dirs.length
      ? `<ul>${sync_dirs.map((d) => `<li>${escapeHtml(d)}</li>`).join("")}</ul>`
      : "None";

  return `<details class="list-item" id="peer-${id}">
            <summary><strong><svg class="lucide lucide-laptop-minimal-icon lucide-laptop-minimal" fill="none" height="20" stroke="currentColor" stroke-linecap="round"
                                 stroke-linejoin="round" stroke-width="2" viewBox="0 0 24 24" width="20"
                                 xmlns="http://www.w3.org/2000/svg">
                        <rect height="12" rx="2" ry="2" width="18" x="3" y="4"/>
                        <line x1="2" x2="22" y1="20" y2="20"/>
                    </svg><span>${hostname}</span></strong
                    ><small class="peer-status">
                    <span>Connected</span>
                    <svg class="lucide lucide-cloud-check-icon lucide-cloud-check connected" fill="none" height="17"
                         stroke="currentColor" stroke-linecap="round"
                         stroke-linejoin="round" stroke-width="2" viewBox="0 0 24 24" width="17"
                         xmlns="http://www.w3.org/2000/svg">
                        <path d="m17 15-5.5 5.5L9 18"/>
                        <path d="M5 17.743A7 7 0 1 1 15.71 10h1.79a4.5 4.5 0 0 1 1.5 8.742"/>
                    </svg>
                </small></summary>
            <p><strong>IP:</strong> ${addr}</p>
            <p><strong>ID:</strong> ${id}</p>
            <p><strong>Instance ID:</strong> ${instance_id ?? "unknown"}</p>
            <p><strong>Last Seen:</strong> ${tsLabel}</p>
            <p><strong>Sync Directories:</strong> ${dirsList}</p>
          </details>`;
}

export function peerDisconnectedStatus() {
  return `<span>Disconnected</span>
                    <svg xmlns="http://www.w3.org/2000/svg" width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-cloud-alert-icon lucide-cloud-alert disconnected"><path d="M12 12v4"/><path d="M12 20h.01"/><path d="M17 18h.5a1 1 0 0 0 0-9h-1.79A7 7 0 1 0 7 17.708"/></svg>`;
}

function insertListItem(listElement, html) {
  const emptyState = listElement.querySelector(".empty-state");
  if (emptyState) {
    emptyState.insertAdjacentHTML("beforebegin", html);
    return;
  }

  listElement.insertAdjacentHTML("beforeend", html);
}

function setEmptyStateVisibility(listElement, itemSelector) {
  const emptyState = listElement?.querySelector(".empty-state");
  if (!emptyState) {
    return;
  }

  emptyState.hidden = listElement.querySelector(itemSelector) !== null;
}

export function updatePeerEmptyState(
  listElement = document.getElementById("peer-list"),
) {
  setEmptyStateVisibility(listElement, 'details[id^="peer-"]');
}

export function updateDirEmptyState(
  listElement = document.getElementById("dir-list"),
) {
  setEmptyStateVisibility(listElement, 'details[id^="dir-"]');
}

export function syncEmptyStates() {
  updatePeerEmptyState();
  updateDirEmptyState();
}

export function addDirToList(dirName, listElement) {
  document.getElementById(`dir-${dirName}`)?.remove();
  insertListItem(listElement, dirListItem(dirName));
  updateDirEmptyState(listElement);
}

export function removeDirFromList(dirName) {
  document.getElementById(`dir-${dirName}`)?.remove();
  updateDirEmptyState();
}

export function addPeerToList(peer, listElement) {
  document.getElementById(`peer-${peer.id}`)?.remove();
  insertListItem(listElement, peerListItem(peer));
  updatePeerEmptyState(listElement);
}

export function setPeerAsDisconnected(peer) {
  const el = document.getElementById(`peer-${peer.id}`);
  const status = el?.querySelector(".peer-status");
  if (status) {
    status.innerHTML = peerDisconnectedStatus();
  }
}

const ACTIVITY_HISTORY_LIMIT = 5;

function getActivityStrip(dir) {
  const el = document.getElementById(`dir-${dir}`);
  if (!el) return null;
  let strip = el.querySelector(".dir-activity");
  if (!strip) {
    strip = document.createElement("div");
    strip.className = "dir-activity";
    strip.hidden = true;
    el.insertBefore(strip, el.querySelector(".dir-actions") ?? null);
  }
  return strip;
}

function ensureActiveSlot(strip) {
  let slot = strip.querySelector(".dir-activity-current");
  if (!slot) {
    slot = document.createElement("div");
    slot.className = "dir-activity-current";
    strip.prepend(slot);
  }
  return slot;
}

function ensureHistory(strip) {
  let hist = strip.querySelector(".dir-activity-history");
  if (!hist) {
    hist = document.createElement("ul");
    hist.className = "dir-activity-history";
    strip.append(hist);
  }
  return hist;
}

function peerLabel(peerId) {
  const peerEl = document.getElementById(`peer-${peerId}`);
  const hostnameSpan = peerEl?.querySelector("summary strong span");
  const text = hostnameSpan?.textContent?.trim();
  return text && text.length > 0 ? text : peerId;
}

function activeKey(path, peer) {
  return `${peer}::${path}`;
}

function pushHistory(strip, html) {
  const hist = ensureHistory(strip);
  const item = document.createElement("li");
  item.innerHTML = html;
  hist.prepend(item);
  while (hist.children.length > ACTIVITY_HISTORY_LIMIT) {
    hist.removeChild(hist.lastChild);
  }
}

export function setSyncStarted({ dir, relative_path, peer }) {
  const strip = getActivityStrip(dir);
  if (!strip) return;
  strip.hidden = false;
  const slot = ensureActiveSlot(strip);
  slot.dataset.key = activeKey(relative_path, peer);
  slot.innerHTML =
    `<span class="sync-active">Syncing <code>${escapeHtml(relative_path)}</code> ` +
    `from <strong>${escapeHtml(peerLabel(peer))}</strong>&hellip;</span>`;
}

export function setSyncCompleted({ dir, relative_path, peer }) {
  const strip = getActivityStrip(dir);
  if (!strip) return;
  strip.hidden = false;
  const slot = strip.querySelector(".dir-activity-current");
  if (slot && slot.dataset.key === activeKey(relative_path, peer)) {
    slot.remove();
  }
  pushHistory(
    strip,
    `<span class="sync-done">synced <code>${escapeHtml(relative_path)}</code></span>`,
  );
}

export function setSyncFailed({ dir, relative_path, peer, reason }) {
  const strip = getActivityStrip(dir);
  if (!strip) return;
  strip.hidden = false;
  const slot = strip.querySelector(".dir-activity-current");
  if (slot && slot.dataset.key === activeKey(relative_path, peer)) {
    slot.remove();
  }
  pushHistory(
    strip,
    `<span class="sync-failed">failed <code>${escapeHtml(relative_path)}</code>` +
      ` &mdash; ${escapeHtml(reason)}</span>`,
  );
}

function basename(path) {
  const p = String(path);
  const i = p.lastIndexOf("/");
  return i >= 0 ? p.slice(i + 1) : p;
}

function conflictItem({ conflict_path, original_path, abs_path }) {
  const safePath = escapeHtml(conflict_path);
  return `<div class="list-item conflict-item">
            <div class="conflict-info">
              <strong>${escapeHtml(basename(original_path))}</strong>
              <span class="conflict-copy">copy: <code>${escapeHtml(basename(conflict_path))}</code></span>
              <span class="conflict-path">${escapeHtml(abs_path)}</span>
            </div>
            <div class="conflict-actions">
              <button class="btn keep-mine-btn" data-conflict="${safePath}" type="button">Keep current</button>
              <button class="btn btn-success use-theirs-btn" data-conflict="${safePath}" type="button">Use this copy</button>
            </div>
          </div>`;
}

function conflictGroup({ dir, items }) {
  const body = (items || []).map(conflictItem).join("");
  return `<div class="conflict-group"><h3>${escapeHtml(dir)}</h3>${body}</div>`;
}

export function renderConflicts(data) {
  const container = document.getElementById("conflicts-container");
  const list = document.getElementById("conflict-list");
  if (!container || !list) return;

  const groups = (data && data.conflicts) || [];
  if (groups.length === 0) {
    list.innerHTML = "";
    container.hidden = true;
    return;
  }

  list.innerHTML = groups.map(conflictGroup).join("");
  container.hidden = false;
}

export async function refreshConflicts() {
  const result = await requestApi("/api/conflicts", {}, {
    statusMessages: {
      500: "Could not load conflicts.",
    },
  });

  if (!result.ok) {
    return false;
  }

  try {
    renderConflicts(await result.response.json());
    return true;
  } catch (err) {
    console.error("Failed to load conflicts:", err);
    showApiError("Could not read Synche response.");
    return false;
  }
}
