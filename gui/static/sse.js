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

    el_peer_list.insertAdjacentHTML("beforeend", component);
}

function disconnect_peer(id) {
    const el = document.getElementById(`peer-${id}`);
    if (!el) return;

    const status = el.querySelector(".peer-status");
    if (status) {
        status.innerHTML = `<span>Disconnected</span>
                    <svg xmlns="http://www.w3.org/2000/svg" width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="lucide lucide-cloud-alert-icon lucide-cloud-alert disconnected"><path d="M12 12v4"/><path d="M12 20h.01"/><path d="M17 18h.5a1 1 0 0 0 0-9h-1.79A7 7 0 1 0 7 17.708"/></svg>`;
    }
}
