import { addDirToList, removeDirFromList, refreshConflicts } from './components.js';
import { clearInlineError, requestApi } from './api_feedback.js';

const el_dir_form = document.getElementById("add-dir-form");
const el_dir_list = document.getElementById("dir-list");
const el_remove_dialog = document.getElementById("remove-dir-dialog");
const el_remove_dir_name = document.getElementById("remove-dir-name");
const el_confirm_remove_btn = document.getElementById("confirm-remove-btn");
const el_home_path_form = document.getElementById("home-path-form");
const el_conflict_list = document.getElementById("conflict-list");
const el_add_dir_error = document.getElementById("add-dir-error");
const el_remove_dir_error = document.getElementById("remove-dir-error");
const el_home_path_error = document.getElementById("home-path-error");

el_dir_form.addEventListener("submit", async (e) => {
  e.preventDefault();

  const data = new FormData(el_dir_form);
  const dir_name = String(data.get("dir-name") ?? "").trim();
  const submitBtn = el_dir_form.querySelector('button[type="submit"]');
  submitBtn.disabled = true;

  const result = await requestApi(`/api/add-sync-dir?name=${encodeURIComponent(dir_name)}`, {
    method: "POST",
  }, {
    expectedStatus: 201,
    inlineError: el_add_dir_error,
    statusMessages: {
      409: "A directory with that name is already synced.",
      500: "Could not add the sync directory.",
    },
  });

  submitBtn.disabled = false;

  if (result.ok) {
    addDirToList(dir_name, el_dir_list);
    el_dir_form.closest("dialog").close();
    el_dir_form.reset();
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
  clearInlineError(el_remove_dir_error);
  el_remove_dialog.showModal();

  el_confirm_remove_btn.onclick = async () => {
    el_confirm_remove_btn.disabled = true;
    const result = await requestApi(
      `/api/remove-sync-dir?name=${encodeURIComponent(dir_name)}`,
      {
        method: "POST",
      },
      {
        expectedStatus: 200,
        inlineError: el_remove_dir_error,
        statusMessages: {
          500: "Could not stop syncing this directory.",
        },
      },
    );
    el_confirm_remove_btn.disabled = false;

    if (result.ok) {
      removeDirFromList(dir_name);
      el_remove_dialog.close();
    }
  };
}

el_home_path_form.addEventListener("submit", async (e) => {
  e.preventDefault();

  const data = new FormData(el_home_path_form);
  const home_path = String(data.get("home-path") ?? "").trim();
  const submitBtn = el_home_path_form.querySelector('button[type="submit"]');
  submitBtn.disabled = true;

  const result = await requestApi(
    `/api/set-home-path?path=${encodeURIComponent(home_path)}`,
    {
      method: "POST",
    },
    {
      expectedStatus: 200,
      inlineError: el_home_path_error,
      statusMessages: {
        400: "Choose an existing directory path.",
        500: "Could not change the home directory.",
      },
    },
  );

  submitBtn.disabled = false;

  if (result.ok) {
    el_home_path_form.closest("dialog").close();
  }
});

el_conflict_list?.addEventListener("click", async (e) => {
  const mineBtn = e.target.closest(".keep-mine-btn");
  const theirsBtn = e.target.closest(".use-theirs-btn");
  const btn = mineBtn || theirsBtn;
  if (!btn) return;

  const path = btn.dataset.conflict;
  if (!path) return;

  const action = mineBtn ? "keep-mine" : "keep-theirs";
  btn.disabled = true;

  const result = await requestApi(
    `/api/resolve-conflict?path=${encodeURIComponent(path)}&action=${action}`,
    { method: "POST" },
    {
      expectedStatus: 200,
      statusMessages: {
        400: "This conflict cannot be resolved.",
        404: "This conflict has already been resolved.",
        500: "Could not resolve the conflict.",
      },
    },
  );

  if (result.ok) {
    await refreshConflicts();
  }

  btn.disabled = false;
});

// Populate the Conflicts section on first paint; SSE keeps it live after.
refreshConflicts();
