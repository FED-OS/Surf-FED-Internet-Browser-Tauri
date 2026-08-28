// Surf FED Dark Reader - content script
// Injects a dark-mode stylesheet into every page.
// Avoids inverting images and videos so they keep their original colors.

(function () {
  const STYLE_ID = 'surf-fed-dark-reader-style';

  function inject() {
    if (document.getElementById(STYLE_ID)) return;

    const css = `
      html {
        filter: invert(0.92) hue-rotate(180deg) brightness(105%) contrast(90%) !important;
        background: #fff !important;
      }
      img, picture, video, iframe, canvas, svg, embed, object {
        filter: invert(1) hue-rotate(180deg) !important;
      }
      * {
        text-shadow: none !important;
        box-shadow: none !important;
      }
      /* Keep common dark sites from being double-inverted */
      html[data-surf-fed-skip] {
        filter: none !important;
      }
    `;

    const style = document.createElement('style');
    style.id = STYLE_ID;
    style.textContent = css;
    (document.head || document.documentElement).appendChild(style);
  }

  // Inject as early as possible, and again when the DOM is ready.
  inject();
  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', inject);
  }
})();
