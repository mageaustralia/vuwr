// The redirect, and the switch that turns it off.
//
// One dynamic rule rather than a static one, because the rule has to
// carry this extension's own URL and that is only known at run time.

const RULE_ID = 1;

// Files vuwr can read. Query strings and fragments are allowed after the
// extension, since a feed URL nearly always carries one.
const PATTERN = "^https?://[^?#]*\\.(csv|tsv|json|xml)([?#].*)?$";

/** Turn the redirect on or off, and say which on the badge. */
async function setEnabled(on) {
  await chrome.storage.local.set({ enabled: on });
  await chrome.declarativeNetRequest.updateDynamicRules({
    removeRuleIds: [RULE_ID],
    addRules: on
      ? [
          {
            id: RULE_ID,
            priority: 1,
            action: {
              type: "redirect",
              redirect: {
                // `\0` is the whole matched URL, handed to the viewer to
                // fetch. The viewer runs on this extension's origin and
                // holds its host permissions, so the fetch is not subject
                // to the file's own CORS policy — which is the whole
                // reason this is an extension and not a bookmarklet.
                regexSubstitution:
                  chrome.runtime.getURL("viewer.html") + "?url=\\0",
              },
            },
            condition: {
              regexFilter: PATTERN,
              // Only a navigation. Fetches a page makes for itself are
              // its own business.
              resourceTypes: ["main_frame"],
            },
          },
        ]
      : [],
  });
  await chrome.action.setBadgeText({ text: on ? "" : "off" });
  await chrome.action.setBadgeBackgroundColor({ color: "#8A5A1E" });
}

chrome.runtime.onInstalled.addListener(async () => {
  const { enabled } = await chrome.storage.local.get("enabled");
  await setEnabled(enabled !== false);
});

chrome.runtime.onStartup.addListener(async () => {
  const { enabled } = await chrome.storage.local.get("enabled");
  await setEnabled(enabled !== false);
});

// The popup asks for the toggle, since the button now opens the popup —
// which is where "it isn't working" gets an answer rather than a shrug.
chrome.runtime.onMessage.addListener((message, _sender, respond) => {
  if (message?.toggle) {
    chrome.storage.local.get("enabled").then(({ enabled }) =>
      setEnabled(enabled === false).then(() => respond(true))
    );
    // Kept open for the async reply.
    return true;
  }
  return false;
});
