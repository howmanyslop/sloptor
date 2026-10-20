import type { AstroIntegration } from "astro";

function injectContextualMenuScript(): string {
	return `
(function () {
  function markdownUrlForPath(pathname) {
    var path = pathname || "/";
    if (!path.endsWith("/")) {
      path += "/";
    }
    return path + "index.md";
  }

  function addContextualMenu() {
    var target = document.querySelector("main .content-panel h1, main h1");
    if (!target) return;

    // Skip clipped / hidden title panels (e.g. hero pages collapse the Starlight title).
    var panel = target.closest(".content-panel");
    if (panel && getComputedStyle(panel).display === "none") return;

    var h1 = target;
    if (h1.dataset.contextualMenu === "true") return;
    if (h1.closest(".page-title-row")) {
      h1.dataset.contextualMenu = "true";
      return;
    }

    h1.dataset.contextualMenu = "true";

    var row = document.createElement("div");
    row.className = "page-title-row";

    var container = document.createElement("span");
    container.className = "contextual-menu";
    container.setAttribute("role", "group");
    container.setAttribute("aria-label", "Page actions");

    var copyButton = document.createElement("button");
    copyButton.type = "button";
    copyButton.className = "contextual-menu__button";
    copyButton.setAttribute("title", "Copy page markdown");
    copyButton.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path></svg><span>Copy page</span>';
    var reduceMotion = window.matchMedia("(prefers-reduced-motion: reduce)");

    // Swap the label at once, then blur it in, so the change reads as one word becoming another.
    function setCopyLabel(text) {
      var label = copyButton.querySelector("span");
      if (!label) return;
      if (!copyButton.style.minWidth) copyButton.style.minWidth = copyButton.offsetWidth + "px";
      label.textContent = text;
      if (typeof label.animate !== "function") return;
      label.animate(
        reduceMotion.matches
          ? [{ opacity: 0.6 }, { opacity: 1 }]
          : [
              { filter: "blur(2px)", opacity: 0.6 },
              { filter: "blur(0)", opacity: 1 },
            ],
        { duration: 150, easing: "cubic-bezier(0.23, 1, 0.32, 1)" },
      );
    }

    copyButton.addEventListener("click", function () {
      var markdownUrl = markdownUrlForPath(window.location.pathname);
      fetch(markdownUrl)
        .then(function (res) {
          if (!res.ok) throw new Error("Failed to fetch page markdown");
          return res.text();
        })
        .then(function (text) {
          return navigator.clipboard.writeText(text);
        })
        .then(function () {
          copyButton.setAttribute("title", "Copied!");
          setCopyLabel("Copied!");
          window.setTimeout(function () {
            copyButton.setAttribute("title", "Copy page markdown");
            setCopyLabel("Copy page");
          }, 2000);
        })
        .catch(function () {
          setCopyLabel("Failed");
          window.setTimeout(function () {
            setCopyLabel("Copy page");
          }, 2000);
        });
    });

    var viewButton = document.createElement("a");
    viewButton.className = "contextual-menu__button contextual-menu__link";
    viewButton.setAttribute("title", "View as Markdown");
    viewButton.href = markdownUrlForPath(window.location.pathname);
    viewButton.innerHTML = '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"></path><polyline points="14 2 14 8 20 8"></polyline><line x1="16" y1="13" x2="8" y2="13"></line><line x1="16" y1="17" x2="8" y2="17"></line><polyline points="10 9 9 9 8 9"></polyline></svg><span>View as Markdown</span>';

    container.appendChild(copyButton);
    container.appendChild(viewButton);

    var parent = h1.parentNode;
    if (!parent) return;
    parent.insertBefore(row, h1);
    row.appendChild(h1);
    row.appendChild(container);
  }

  document.addEventListener("astro:page-load", addContextualMenu);
  addContextualMenu();
})();
`;
}

export default function createContextualMenuIntegration(): AstroIntegration {
	return {
		name: "contextual-menu",
		hooks: {
			"astro:config:setup": ({ injectScript }): void => {
				injectScript("page", injectContextualMenuScript());
			},
		},
	};
}
