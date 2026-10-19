// The site's one script: the theme toggle, the docs menu on narrow screens,
// and the copy buttons on code blocks. Everything works without it except
// those three.
(function () {
  var root = document.documentElement;
  var STORAGE_KEY = 'prepcook-theme';
  var darkQuery = window.matchMedia('(prefers-color-scheme: dark)');

  function effectiveTheme() {
    var explicit = root.getAttribute('data-theme');
    if (explicit === 'light' || explicit === 'dark') return explicit;
    return darkQuery.matches ? 'dark' : 'light';
  }

  // <picture> sources pick the -dark image through prefers-color-scheme,
  // which follows the system, not the toggle. When the reader has chosen a
  // theme, each dark source is forced on or off to match it.
  function syncImages() {
    var explicit = root.getAttribute('data-theme');
    var sources = document.querySelectorAll('picture source[media]');
    sources.forEach(function (source) {
      var original = source.getAttribute('data-media');
      if (original === null) {
        original = source.getAttribute('media');
        if (original.indexOf('prefers-color-scheme: dark') === -1) return;
        source.setAttribute('data-media', original);
      }
      var media = original;
      if (explicit === 'dark') media = 'all';
      if (explicit === 'light') media = 'not all';
      if (source.getAttribute('media') !== media) {
        source.setAttribute('media', media);
      }
    });
    var dark = effectiveTheme() === 'dark';
    document.querySelectorAll('a[data-dark-href]').forEach(function (link) {
      if (!link.hasAttribute('data-light-href')) {
        link.setAttribute('data-light-href', link.getAttribute('href'));
      }
      link.setAttribute(
        'href',
        link.getAttribute(dark ? 'data-dark-href' : 'data-light-href')
      );
    });
  }

  function syncToggle() {
    var next = effectiveTheme() === 'dark' ? 'light' : 'dark';
    document.querySelectorAll('.theme-toggle').forEach(function (button) {
      button.setAttribute('aria-label', 'Switch to the ' + next + ' theme');
    });
  }

  function setTheme(theme) {
    root.setAttribute('data-theme', theme);
    try {
      localStorage.setItem(STORAGE_KEY, theme);
    } catch (e) {
      // Storage can be unavailable (private windows, blocked site data);
      // the choice then lasts for this page only.
    }
    syncImages();
    syncToggle();
  }

  document.querySelectorAll('.theme-toggle').forEach(function (button) {
    button.addEventListener('click', function () {
      setTheme(effectiveTheme() === 'dark' ? 'light' : 'dark');
    });
  });

  var onSystemChange = function () {
    syncImages();
    syncToggle();
  };
  if (darkQuery.addEventListener) {
    darkQuery.addEventListener('change', onSystemChange);
  }
  syncImages();
  syncToggle();

  // Docs menu: below the sidebar breakpoint the sidebar is a drawer.
  var menuButton = document.querySelector('.menu-button');
  var scrim = document.querySelector('.scrim');
  function setMenu(open) {
    document.body.classList.toggle('nav-open', open);
    if (menuButton) {
      menuButton.setAttribute('aria-expanded', open ? 'true' : 'false');
      menuButton.setAttribute(
        'aria-label',
        open ? 'Close the docs menu' : 'Open the docs menu'
      );
    }
    if (scrim) scrim.hidden = !open;
  }
  if (menuButton) {
    menuButton.addEventListener('click', function () {
      setMenu(!document.body.classList.contains('nav-open'));
    });
  }
  if (scrim) {
    scrim.addEventListener('click', function () {
      setMenu(false);
    });
  }
  document.addEventListener('keydown', function (event) {
    if (
      event.key === 'Escape' &&
      document.body.classList.contains('nav-open')
    ) {
      setMenu(false);
      if (menuButton) menuButton.focus();
    }
  });

  // Copy buttons on code blocks.
  document.querySelectorAll('.code-block').forEach(function (block) {
    var button = block.querySelector('.copy');
    var code = block.querySelector('pre code');
    if (!button || !code || !navigator.clipboard) return;
    button.hidden = false;
    button.addEventListener('click', function () {
      navigator.clipboard.writeText(code.textContent.replace(/\n$/, '')).then(
        function () {
          button.textContent = 'Copied';
          setTimeout(function () {
            button.textContent = 'Copy';
          }, 1500);
        },
        function () {
          button.textContent = 'Copy failed';
        }
      );
    });
  });
})();
