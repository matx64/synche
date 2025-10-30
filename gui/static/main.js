const el_dir_form = document.getElementById("add-dir-form");
const el_dir_list = document.getElementById("dir-list");

el_dir_form.addEventListener("submit", async (e) => {
    e.preventDefault();

    const data = new FormData(el_dir_form);
    const dir_name = data.get("dir-name");

    el_dir_form.closest("dialog").close();

    const res = await fetch(`/api/add-sync-dir?name=${dir_name}`, {
        method: "POST",
    });

    if (res.status == 201) {
        el_dir_list.insertAdjacentHTML(
            "beforeend",
            dir_list_item_component(dir_name)
        );
    }
});

el_dir_list.addEventListener("click", async (e) => {
    if (e.target.matches(".remove-dir-btn")) {
        const dir_id = e.target.closest("details")?.id ?? null;
        const prefix = "dir-";

        if (dir_id && dir_id.startsWith(prefix)) {
            await delete_dir(dir_id.slice(prefix.length));
        }
    }
});

async function delete_dir(dir_name) {
    const confirmed = confirm(`Stop syncing "${dir_name}" directory?`);

    if (confirmed) {
        const res = await fetch(`/api/remove-sync-dir?name=${dir_name}`, {
            method: "POST",
        });

        if (res.status == 200) {
            document.getElementById(`dir-${dir_name}`).remove();
        }
    }
}

function dir_list_item_component(name) {
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
                    </strong><small>Up to Date</small>
            </summary>

            <div class="dir-actions">
              <button class="btn remove-dir-btn">Stop Syncing</button>
            </div>
          </details>`;
}
