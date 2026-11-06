// The conversation runs on a simulation clock that only advances while
// the stage is playing and on screen. Each beat adds a message, shows
// Tammy typing, or moves the card she filed. Under reduced motion the
// whole conversation is shown at once and Play replays it.
(function () {
  var reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  var MARK =
    '<picture><source media="(prefers-color-scheme: dark)" srcset="assets/tammy-mark-dark.svg" /><img src="assets/tammy-mark.svg" alt="" width="24" height="24" /></picture>';
  var BOOK =
    '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true"><path d="M2.5 3.5c2-.8 3.8-.8 5.5.6v9c-1.7-1.4-3.5-1.4-5.5-.6zM13.5 3.5c-2-.8-3.8-.8-5.5.6v9c1.7-1.4 3.5-1.4 5.5-.6z" /></svg>';
  var PEOPLE = {
    maya: { name: 'Maya', cls: 'cust', initial: 'M' },
    tammy: { name: 'Tammy', cls: 'tammy', tag: 'App' },
    sam: { name: 'Sam', cls: 'team', initial: 'S' },
  };
  var BEATS = [
    {
      at: 500,
      to: 'cust',
      who: 'maya',
      time: '10:02',
      html: '<p>Hi, how do I share a list with my partner?</p>',
    },
    { at: 1300, to: 'cust', typing: true },
    {
      at: 3300,
      to: 'cust',
      who: 'tammy',
      time: '10:02',
      tools: ['search_menu'],
      html:
        '<p>Open the list, choose Share, and type their email address. They see the list the next time they open Pantry, and you can both add and tick items.</p><span class="cite">' +
        BOOK +
        'From the menu: Sharing a list with someone</span>',
    },
    {
      at: 6200,
      to: 'cust',
      who: 'maya',
      time: '10:04',
      html: '<p>Done, thanks. But when he ticks things off at the shop, they still show as to buy on my phone.</p>',
    },
    { at: 7000, to: 'cust', typing: true },
    {
      at: 9200,
      to: 'cust',
      who: 'tammy',
      time: '10:05',
      tools: ['search_menu', 'ask_customer'],
      html: '<p>Sorry about that. So I can pass it on properly:</p><ol><li>Which list is it?</li><li>Do the ticks show up if you close and reopen the app?</li><li>Are you both on phones, or is one of you on the website?</li></ol>',
    },
    {
      at: 12800,
      to: 'cust',
      who: 'maya',
      time: '10:06',
      html: '<p>The Weekend list. Reopening shows them. We’re both on phones.</p>',
    },
    { at: 13600, to: 'cust', typing: true },
    {
      at: 15400,
      to: 'cust',
      who: 'tammy',
      time: '10:07',
      tools: ['file_investigation'],
      html: '<p>Thank you, that’s what the team needs. I’ve passed it on with your steps and I’ll come back to this thread when they’ve looked.</p>',
      card: 'filed',
    },
    { at: 17000, card: 'investigating' },
    { at: 25500, card: 'triaged' },
    {
      at: 26000,
      to: 'team',
      who: 'tammy',
      time: '10:38',
      html: '<p>DEF-3 is triaged: ticks on one phone reach the other only when the list is reopened. The report is on the card.</p>',
    },
    { at: 26800, to: 'cust', typing: true },
    {
      at: 29000,
      to: 'cust',
      who: 'tammy',
      time: '10:39',
      tools: ['check_order'],
      html: '<p>The team has looked into it. When one of you ticks an item, the other phone only picks up the change the next time the list is opened, so ticks made at the shop don’t show until then. It isn’t something in your setup. In the meantime, reopening the list shows the latest ticks. The team has the details.</p>',
    },
    {
      at: 33000,
      to: 'team',
      who: 'sam',
      time: '10:41',
      html: '<p><code>/tammy convert DEF-3</code></p>',
    },
    {
      at: 34200,
      to: 'team',
      who: 'tammy',
      time: '10:41',
      html: '<p>DEF-3 is now a bug.</p>',
      card: 'bug',
    },
    { at: 43000, end: true },
  ];

  var stage = document.getElementById('stage');
  var panes = {
    cust: document.getElementById('cust'),
    team: document.getElementById('team'),
  };
  var cols = {};
  document.querySelectorAll('#cols .col').forEach(function (c) {
    cols[c.dataset.col] = c;
  });
  var playBtn = document.getElementById('play');
  var toggle = document.getElementById('tools-toggle');
  var need = document.getElementById('need');
  var simNow,
    next,
    typingEl,
    card,
    cardEl,
    since,
    lastFrame = null,
    userPaused = false,
    offscreen = false,
    resetting = false;
  var lastMinute = 0;

  function scrollDown(pane) {
    pane.scrollTo({
      top: pane.scrollHeight,
      behavior: reduce ? 'auto' : 'smooth',
    });
  }
  function avatar(p) {
    return (
      '<div class="avatar ' +
      p.cls +
      '">' +
      (p.cls === 'tammy' ? MARK : p.initial) +
      '</div>'
    );
  }
  function addMessage(b, instant) {
    var p = PEOPLE[b.who];
    var el = document.createElement('div');
    el.className = 'msg';
    var tools = b.tools
      ? '<div class="tools">' +
        b.tools
          .map(function (t) {
            return '<span class="tool">' + t + '</span>';
          })
          .join('') +
        '</div>'
      : '';
    el.innerHTML =
      avatar(p) +
      '<div class="body"><div class="who">' +
      p.name +
      (p.tag ? ' <em>' + p.tag + '</em>' : '') +
      ' <time>' +
      b.time +
      '</time></div>' +
      b.html +
      tools +
      '</div>';
    var empty = panes[b.to].querySelector('[data-empty]');
    if (empty) empty.remove();
    panes[b.to].appendChild(el);
    if (!instant && !reduce)
      el.animate(
        [
          { opacity: 0, transform: 'translateY(6px)' },
          { opacity: 1, transform: 'none' },
        ],
        { duration: 360, easing: 'cubic-bezier(0.16, 1, 0.3, 1)' }
      );
    scrollDown(panes[b.to]);
  }
  function setTyping(pane) {
    if (typingEl) typingEl.remove();
    typingEl = null;
    if (!pane) return;
    typingEl = document.createElement('div');
    typingEl.className = 'msg';
    typingEl.innerHTML =
      avatar(PEOPLE.tammy) +
      '<div class="body"><div class="who">Tammy <em>App</em></div><span class="typing" aria-label="Tammy is typing"><i></i><i></i><i></i></span></div>';
    panes[pane].appendChild(typingEl);
    scrollDown(panes[pane]);
  }

  function cardHtml() {
    var chip =
      card === 'bug'
        ? '<span class="chip bug">bug</span>'
        : '<span class="chip inv">investigation</span>';
    var strip = '';
    var status = '';
    var actions = '';
    if (card === 'filed')
      status = '<div class="k-status idle">Filed by Tammy from #help</div>';
    if (card === 'investigating')
      status =
        '<div class="k-status working"><span class="dot"></span>Investigating · ' +
        Math.max(1, Math.round((simNow - since) / 300)) +
        ' min</div>';
    if (card === 'triaged') {
      strip =
        '<div class="k-strip"><span class="dot amber"></span>Needs you</div>';
      status =
        '<div class="k-status attn"><span class="dot amber"></span>Triaged — report ready</div>';
      actions =
        '<div class="report"><b>TRIAGE REPORT</b><dl><dt>What</dt><dd>Ticks save, but the other phone re-reads the list only when it opens.</dd><dt>Where</dt><dd>List sync, mobile app</dd><dt>Fix for now</dt><dd>Reopen the list</dd><dt>Confidence</dt><dd>High</dd></dl></div>' +
        '<div class="k-actions"><span class="fbtn primary">Convert to bug</span><span class="fbtn">Close</span></div>';
    }
    if (card === 'bug')
      status =
        '<div class="k-status working"><span class="dot"></span>Writing the fix · ' +
        Math.max(1, Math.round((simNow - since) / 300)) +
        ' min</div>';
    return (
      strip +
      '<div class="k-body"><div class="k-top"><span class="k-key">DEF-3</span>' +
      chip +
      '</div><p class="k-title">Items my partner ticked off still show as to buy</p>' +
      status +
      actions +
      '</div>'
    );
  }
  function renderCard(animate) {
    if (!card) return;
    var target = card === 'bug' ? cols.execute : cols.plan;
    var first =
      cardEl && cardEl.parentNode ? cardEl.getBoundingClientRect() : null;
    if (!cardEl) {
      cardEl = document.createElement('article');
      cardEl.className = 'k';
    }
    var html = cardHtml();
    if (cardEl._html !== html) {
      cardEl.innerHTML = html;
      cardEl._html = html;
    }
    cardEl.classList.toggle('need', card === 'triaged');
    if (cardEl.parentNode !== target) target.appendChild(cardEl);
    need.hidden = card !== 'triaged';
    // A board shorter than the triaged card scrolls to its actions.
    var colsEl = target.parentNode;
    if (card === 'triaged' && colsEl.scrollHeight > colsEl.clientHeight + 2)
      colsEl.scrollTo({
        top: colsEl.scrollHeight,
        behavior: reduce ? 'auto' : 'smooth',
      });
    if (card !== 'triaged') colsEl.scrollTo({ top: 0 });
    Object.keys(cols).forEach(function (k) {
      cols[k].querySelector('[data-count]').textContent =
        cols[k].querySelectorAll('.k').length;
    });
    if (!animate || reduce) return;
    if (!first) {
      cardEl.animate(
        [
          { opacity: 0, transform: 'translateY(-10px)' },
          { opacity: 1, transform: 'none' },
        ],
        { duration: 500, easing: 'cubic-bezier(0.16, 1, 0.3, 1)' }
      );
      return;
    }
    var to = cardEl.getBoundingClientRect();
    var dx = first.left - to.left;
    var dy = first.top - to.top;
    if (Math.abs(dx) > 1 || Math.abs(dy) > 1)
      cardEl.animate(
        [
          { transform: 'translate(' + dx + 'px,' + dy + 'px)' },
          { transform: 'none' },
        ],
        { duration: 720, easing: 'cubic-bezier(0.16, 1, 0.3, 1)' }
      );
  }

  function apply(b, instant) {
    if (b.typing) return setTyping(instant ? null : b.to);
    if (b.who) {
      setTyping(null);
      addMessage(b, instant);
    }
    if (b.card) {
      card = b.card;
      since = simNow;
      renderCard(!instant);
    }
  }

  // A card already in the kitchen, so the board reads as a working board.
  var context = document.createElement('article');
  context.className = 'k';
  context.innerHTML =
    '<div class="k-body"><div class="k-top"><span class="k-key">DEF-4</span><span class="chip bug">bug</span></div><p class="k-title">Quantity 0 is saved as 1 without a word</p><div class="k-status working"><span class="dot"></span>Writing the fix</div></div>';

  function reset() {
    simNow = 0;
    next = 0;
    card = null;
    if (cardEl) cardEl.remove();
    cardEl = null;
    setTyping(null);
    panes.cust.innerHTML = '';
    panes.team.innerHTML =
      '<p class="empty" data-empty>She posts a short summary here when a report is triaged.</p>';
    need.hidden = true;
    cols.execute.appendChild(context);
    Object.keys(cols).forEach(function (k) {
      cols[k].querySelector('[data-count]').textContent =
        cols[k].querySelectorAll('.k').length;
    });
  }
  function showAll() {
    reset();
    BEATS.forEach(function (b) {
      if (!b.end) {
        simNow = b.at;
        apply(b, true);
      }
    });
    next = BEATS.length;
  }

  function frame(t) {
    var dt = lastFrame === null ? 0 : Math.min(t - lastFrame, 100);
    lastFrame = t;
    if (!userPaused && !offscreen && !resetting) {
      simNow += dt;
      while (next < BEATS.length && simNow >= BEATS[next].at) {
        var b = BEATS[next++];
        if (b.end) {
          resetting = true;
          document.getElementById('clear').classList.add('show');
          setTimeout(function () {
            reset();
            document.getElementById('clear').classList.remove('show');
            resetting = false;
          }, 900);
        } else apply(b, false);
      }
      var minute = Math.floor(simNow / 300);
      if ((card === 'investigating' || card === 'bug') && minute !== lastMinute)
        renderCard(false);
      lastMinute = minute;
    }
    requestAnimationFrame(frame);
  }

  function sync() {
    var playing = !userPaused && !offscreen;
    document.getAnimations().forEach(function (a) {
      if (stage.contains(a.effect && a.effect.target))
        playing ? a.play() : a.pause();
    });
    playBtn.setAttribute('aria-pressed', userPaused ? 'true' : 'false');
    playBtn.querySelector('span').textContent = userPaused ? 'Play' : 'Pause';
    playBtn.querySelector('svg').innerHTML = userPaused
      ? '<path d="M4.5 3.2v9.6a.6.6 0 0 0 .9.5l7.6-4.8a.6.6 0 0 0 0-1L5.4 2.7a.6.6 0 0 0-.9.5Z" fill="currentColor" />'
      : '<rect x="3.5" y="3" width="3" height="10" rx="1" fill="currentColor" /><rect x="9.5" y="3" width="3" height="10" rx="1" fill="currentColor" />';
  }
  playBtn.addEventListener('click', function () {
    // Under reduced motion the finished conversation is on screen; Play
    // replays it from the start.
    if (userPaused && next >= BEATS.length) reset();
    userPaused = !userPaused;
    sync();
  });
  toggle.addEventListener('click', function () {
    var on = toggle.getAttribute('aria-checked') !== 'true';
    toggle.setAttribute('aria-checked', on ? 'true' : 'false');
    stage.classList.toggle('hide-tools', !on);
  });
  new IntersectionObserver(
    function (entries) {
      offscreen = !entries[0].isIntersecting;
      sync();
    },
    { threshold: 0.15 }
  ).observe(stage);
  document.addEventListener('visibilitychange', function () {
    offscreen = document.hidden;
    sync();
  });

  reset();
  if (reduce) {
    userPaused = true;
    showAll();
  }
  sync();
  requestAnimationFrame(frame);
})();
