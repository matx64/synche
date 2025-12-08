export function dirListItem(name) {
  return `<details class="list-item" id="dir-${name}">
            <summary>
              <strong>
                        <svg class="lucide lucide-folder-open-icon lucide-folder-open" fill="none" height="20"
                             stroke="currentColor" stroke-linecap="round"
                             stroke-linejoin="round" stroke-width="2" viewBox="0 0 24 24" width="20"
                             xmlns="http://www.w3.org/2000/svg">
                            <path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2"/>
                        </svg>
                        <span>${name}</span>
                    </strong>
            </summary>

            <div class="dir-actions">
            <button class="btn icon-btn remove-dir-btn">
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

export function peerListItem({ id, addr, hostname }) {
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
          </details>`;
}

export function peerDisconnectedStatus() {
  return `<span>Disconnected</span>
                    <svg xmlns="http://www.w3.org/2000/svg" width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-cloud-alert-icon lucide-cloud-alert disconnected"><path d="M12 12v4"/><path d="M12 20h.01"/><path d="M17 18h.5a1 1 0 0 0 0-9h-1.79A7 7 0 1 0 7 17.708"/></svg>`;
}

export function addDirToList(dirName, listElement) {
  document.getElementById(`dir-${dirName}`)?.remove();
  listElement.insertAdjacentHTML("beforeend", dirListItem(dirName));
}

export function removeDirFromList(dirName) {
  document.getElementById(`dir-${dirName}`)?.remove();
}

export function addPeerToList(peer, listElement) {
  document.getElementById(`peer-${peer.id}`)?.remove();
  listElement.insertAdjacentHTML("beforeend", peerListItem(peer));
}

export function setPeerAsDisconnected(peer) {
  const el = document.getElementById(`peer-${peer.id}`);
  const status = el?.querySelector(".peer-status");
  if (status) {
    status.innerHTML = peerDisconnectedStatus();
  }
}
