/**
 * Global Connect - the only script, and nothing on any page needs it.
 *
 * ADR 0007 section 3's rule, unchanged from Desk's and from the profiler's
 * before that: **every page is complete without this file.** The navigation is
 * a `<details>`, which opens and closes on its own; the language switcher is
 * another one; there is no form, no fetch and no state.
 *
 * What is left is two pieces of tidying that a `<details>` cannot do for
 * itself, and both of them only ever *close* something. Nothing here reveals
 * content, so a browser that never runs it shows the same site with one menu
 * occasionally left open.
 *
 * It is served from the site's own binary at a hashed path, so the content
 * security policy stays `script-src 'self'` with no `unsafe-inline` and no
 * nonce machinery.
 */

(() => {
    "use strict";

    /** Close every open disclosure except one. */
    const closeOthers = (except) => {
        for (const menu of document.querySelectorAll("details[data-menu], details[data-nav]")) {
            if (menu !== except) {
                menu.open = false;
            }
        }
    };

    // A click outside an open menu closes it. Without this the only way to
    // dismiss one is to click its own summary again, which is not what anybody
    // expects of a dropdown - and is the single thing that makes a `<details>`
    // feel like a menu rather than an accordion.
    document.addEventListener("click", (event) => {
        const inside = event.target instanceof Element
            ? event.target.closest("details[data-menu], details[data-nav]")
            : null;

        closeOthers(inside);
    });

    document.addEventListener("keydown", (event) => {
        if (event.key === "Escape") {
            closeOthers(null);
        }
    });

    // The mobile menu is `md:hidden`, so at a wide viewport its panel is not
    // painted - but the element stays open, and shrinking the window again
    // would show it hanging there. Closing it on the way up is cheaper than
    // reasoning about that.
    const wide = window.matchMedia("(min-width: 48rem)");

    wide.addEventListener("change", (event) => {
        if (event.matches) {
            closeOthers(null);
        }
    });
})();
