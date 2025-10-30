const el_peer_list = document.getElementById("peer-list");

const es = new EventSource("/api/events");

es.onmessage = (event) => {
    const data = JSON.parse(event.data);

    console.log("SSE:", data);

    const [kind, payload] = Object.entries(data)[0];

    switch (kind) {
        case "PeerConnected":
            render_peer_list_item_component(payload);
            break;

        case "PeerDisconnected":
            disconnect_peer(payload);
            break;
    }
};

function render_peer_list_item_component({id, addr, hostname}) {
    document.getElementById(`peer-${id}`)?.remove();

    const component = `<details class="list-item" id="peer-${id}">
            <summary><strong><svg class="lucide lucide-laptop-minimal-icon lucide-laptop-minimal" fill="none" height="20" stroke="currentColor" stroke-linecap="round"
                                 stroke-linejoin="round" stroke-width="2" viewBox="0 0 24 24" width="20"
                                 xmlns="http://www.w3.org/2000/svg">
                        <rect height="12" rx="2" ry="2" width="18" x="3" y="4"/>
                        <line x1="2" x2="22" y1="20" y2="20"/>
                    </svg><span>${hostname}</span></strong
                    ><small class="peer-status">🟢 Connected</small></summary>
            <p><strong>IP:</strong> ${addr}</p>
            <p><strong>ID:</strong> ${id}</p>
          </details>`;

    el_peer_list.insertAdjacentHTML("beforeend", component);
}

function disconnect_peer(id) {
    const el = document.getElementById(`peer-${id}`);
    if (!el) return;

    const status = el.querySelector(".peer-status");
    if (status) status.innerHTML = "🔴 Disconnected";
}
