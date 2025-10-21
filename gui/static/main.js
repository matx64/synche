// const es = new EventSource("/api/events");

// es.onmessage = (event) => {
//   console.log("New message from server:", event);
// };

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
      el_dir_list.querySelector(`#dir-${dir_name}`).remove();
    }
  }
}

function dir_list_item_component(name) {
  return `<details class="list-item" id="dir-${name}">
            <summary>
              <strong>📂 ${name}</strong><small>Up to Date</small>
            </summary>

            <div class="dir-actions">
              <button class="btn remove-dir-btn">Stop Syncing</button>
            </div>
          </details>`;
}
