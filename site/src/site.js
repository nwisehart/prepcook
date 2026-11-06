// The site's one script: the theme toggle, the docs menu on narrow screens,
// the copy buttons on code blocks, and holding the board film still for
// readers who ask for less motion. Everything works without it except those
// four.
(function () {
  var root = document.documentElement;
  var STORAGE_KEY = 'prepcook-theme';
  var darkQuery = window.matchMedia('(prefers-color-scheme: dark)');

  function effectiveTheme() {
    var explicit = root.getAttribute('data-theme');
    if (explicit === 'light' || explicit === 'dark') return explicit;
    return darkQuery.matches ? 'dark' : 'light';
  }

  // <picture> and <video> sources pick the -dark image or film through
  // prefers-color-scheme, which follows the system, not the toggle. When the
  // reader has chosen a theme, each dark source is forced on or off to match
  // it; the light sources carry no media and follow whichever dark ones are
  // off. A video only rereads its sources when loaded again, and its poster
  // swaps through data-dark-poster.
  function syncImages() {
    var explicit = root.getAttribute('data-theme');
    var sources = document.querySelectorAll(
      'picture source[media], video source[media]'
    );
    var films = [];
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
        var parent = source.parentNode;
        if (parent.tagName === 'VIDEO' && films.indexOf(parent) === -1) {
          films.push(parent);
        }
      }
    });
    var dark = effectiveTheme() === 'dark';
    document
      .querySelectorAll('video[data-dark-poster]')
      .forEach(function (video) {
        if (!video.hasAttribute('data-light-poster')) {
          video.setAttribute('data-light-poster', video.getAttribute('poster'));
        }
        video.setAttribute(
          'poster',
          video.getAttribute(dark ? 'data-dark-poster' : 'data-light-poster')
        );
      });
    films.forEach(function (video) {
      video.load();
    });
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

  // The home page's board film plays on its own unless the reader has asked
  // for less motion; then it waits, paused on its poster, with controls.
  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
    document.querySelectorAll('video[autoplay]').forEach(function (video) {
      video.removeAttribute('autoplay');
      video.pause();
      video.controls = true;
    });
  }

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
