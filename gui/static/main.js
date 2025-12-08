import { addDirToList, removeDirFromList } from './components.js';

const el_dir_form = document.getElementById("add-dir-form");
const el_dir_list = document.getElementById("dir-list");
const el_remove_dialog = document.getElementById("remove-dir-dialog");
const el_remove_dir_name = document.getElementById("remove-dir-name");
const el_confirm_remove_btn = document.getElementById("confirm-remove-btn");
const el_home_path_form = document.getElementById("home-path-form");

el_dir_form.addEventListener("submit", async (e) => {
  e.preventDefault();

  const data = new FormData(el_dir_form);
  const dir_name = data.get("dir-name");

  el_dir_form.closest("dialog").close();

  const res = await fetch(`/api/add-sync-dir?name=${dir_name}`, {
    method: "POST",
  });

  if (res.status == 201) {
    addDirToList(dir_name, el_dir_list);
  }
});

el_dir_list.addEventListener("click", async (e) => {
  const removeBtn = e.target.closest(".remove-dir-btn");
  if (removeBtn) {
    const dir_id = removeBtn.closest("details")?.id ?? null;
    const prefix = "dir-";

    if (dir_id && dir_id.startsWith(prefix)) {
      await delete_dir(dir_id.slice(prefix.length));
    }
  }
});

async function delete_dir(dir_name) {
  el_remove_dir_name.textContent = dir_name;
  el_remove_dialog.showModal();

  el_confirm_remove_btn.onclick = async () => {
    const res = await fetch(`/api/remove-sync-dir?name=${dir_name}`, {
      method: "POST",
    });

    if (res.status == 200) {
      removeDirFromList(dir_name);
      el_remove_dialog.close();
    }
  };
}

el_home_path_form.addEventListener("submit", async (e) => {
  e.preventDefault();

  const data = new FormData(el_home_path_form);
  const home_path = data.get("home-path");

  el_home_path_form.closest("dialog").close();

  await fetch(`/api/set-home-path?path=${encodeURIComponent(home_path)}`, {
    method: "POST",
  });
});
