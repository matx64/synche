const NETWORK_ERROR_MESSAGE = "Could not reach Synche.";
const DEFAULT_ERROR_MESSAGE = "Request failed.";
const TOAST_TIMEOUT_MS = 5000;

const DEFAULT_STATUS_MESSAGES = {
  400: "The request was not valid.",
  404: "The item was not found.",
  409: "The requested change conflicts with the current state.",
  500: "Synche hit an internal error.",
};

function expectedStatuses(expectedStatus) {
  if (expectedStatus == null) {
    return null;
  }

  return Array.isArray(expectedStatus) ? expectedStatus : [expectedStatus];
}

function isExpectedStatus(response, expectedStatus) {
  const expected = expectedStatuses(expectedStatus);
  if (!expected) {
    return response.ok;
  }

  return expected.includes(response.status);
}

function toastRegion() {
  let region = document.getElementById("toast-region");
  if (region) {
    return region;
  }

  region = document.createElement("div");
  region.id = "toast-region";
  region.className = "toast-region";
  region.setAttribute("role", "status");
  region.setAttribute("aria-live", "polite");
  region.setAttribute("aria-atomic", "true");
  document.body.append(region);
  return region;
}

function httpErrorMessage(response, statusMessages) {
  const reason =
    statusMessages?.[response.status] ??
    DEFAULT_STATUS_MESSAGES[response.status] ??
    response.statusText ??
    DEFAULT_ERROR_MESSAGE;

  return `HTTP ${response.status}: ${reason}`;
}

export function clearInlineError(inlineError) {
  if (!inlineError) {
    return;
  }

  inlineError.textContent = "";
  inlineError.hidden = true;
}

export function showToast(message) {
  const toast = document.createElement("div");
  toast.className = "toast";
  toast.setAttribute("role", "alert");
  toast.textContent = message;

  toastRegion().append(toast);

  window.setTimeout(() => {
    toast.remove();
  }, TOAST_TIMEOUT_MS);
}

export function showApiError(message, inlineError) {
  if (inlineError) {
    inlineError.textContent = message;
    inlineError.hidden = false;
  }

  showToast(message);
}

export async function requestApi(
  url,
  options = {},
  { expectedStatus = null, inlineError = null, statusMessages = {} } = {},
) {
  clearInlineError(inlineError);

  let response;
  try {
    response = await fetch(url, options);
  } catch (err) {
    console.error("API request failed:", err);
    showApiError(NETWORK_ERROR_MESSAGE, inlineError);
    return {
      ok: false,
      response: null,
      status: null,
      message: NETWORK_ERROR_MESSAGE,
      networkError: true,
    };
  }

  if (!isExpectedStatus(response, expectedStatus)) {
    const message = httpErrorMessage(response, statusMessages);
    showApiError(message, inlineError);
    return {
      ok: false,
      response,
      status: response.status,
      message,
      networkError: false,
    };
  }

  return {
    ok: true,
    response,
    status: response.status,
    message: null,
    networkError: false,
  };
}
