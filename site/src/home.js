// The board simulation. Cards follow the real stage rules: in Chef mode a
// person drags cards between columns and approves each plan; in Prep mode
// cards move themselves and a plan at high confidence approves itself.
// Merging and clean up are a person's in both. Time runs on a simulation
// clock that only advances while the board is playing and on screen.
(function () {
  var reduce = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  var COLS = [
    {
      id: 'todo',
      name: 'To Do',
      help: 'Waiting for you to send it to Plan',
      dot: 'hollow',
    },
    {
      id: 'plan',
      name: 'Plan',
      help: 'Reproduce it, then explain it',
      dot: '',
    },
    {
      id: 'execute',
      name: 'Execute',
      help: 'The smallest fix, with tests',
      dot: '',
    },
    {
      id: 'review',
      name: 'Review',
      help: 'Fresh eyes on the fix',
      dot: '',
    },
    {
      id: 'shipped',
      name: 'Shipped',
      help: 'PR open or merged; you clean up',
      dot: 'green',
    },
  ];
  // DEF-2 to DEF-6 are the Pantry demo board's own cards; the rest are
  // further Pantry-shaped reports so the queue has something to work.
  var SEED = [
    {
      key: 'DEF-2',
      type: 'bug',
      title: 'Sharing a list fails for a user who exists',
      col: 'plan',
      phase: 'gate',
    },
    {
      key: 'DEF-3',
      type: 'inv',
      title: 'Items my partner ticked off still show as to buy',
      col: 'plan',
      phase: 'planning',
      age: 2500,
    },
    {
      key: 'DEF-4',
      type: 'bug',
      title: 'Quantity 0 is saved as 1 without a word',
      col: 'execute',
      phase: 'writing',
      age: 3000,
    },
    {
      key: 'DEF-5',
      type: 'bug',
      title: 'Renaming a list to a blank name does nothing and says nothing',
      col: 'review',
      phase: 'reviewing',
      age: 1500,
      rounds: 2,
    },
    {
      key: 'DEF-6',
      type: 'bug',
      title: 'Sign-in rejects an email typed with a trailing space',
      col: 'shipped',
      phase: 'open',
      age: 2500,
      pr: 1,
    },
    {
      key: 'DEF-7',
      type: 'bug',
      title: 'Undo after deleting an item restores the wrong one',
      col: 'todo',
      phase: 'filed',
    },
    {
      key: 'DEF-8',
      type: 'bug',
      title: 'The weekly list shows last week’s date on Mondays',
      col: 'todo',
      phase: 'filed',
    },
    {
      key: 'DEF-9',
      type: 'bug',
      title: 'Search misses items with an accent in the name',
      col: 'todo',
      phase: 'filed',
      rounds: 2,
    },
  ];
  var LATER = [
    {
      at: 16000,
      key: 'DEF-10',
      type: 'bug',
      title: 'Moving an item to another list drops its quantity',
    },
    {
      at: 34000,
      key: 'DEF-11',
      type: 'bug',
      title: 'Clearing a checked item leaves a gap in the list',
    },
  ];
  var T = {
    read: 6500,
    planning: 6500,
    inv: 5500,
    selfApprove: 1600,
    writing: 7000,
    readyAuto: 1300,
    reviewing: 5000,
    fixing: 3200,
    approvedAuto: 1300,
    open: 5000,
    filedAuto: 1800,
  };

  var app = document.getElementById('app');
  var colsEl = document.getElementById('cols');
  var cursor = document.getElementById('cursor');
  var playBtn = document.getElementById('play');
  var mode = 'chef';
  var userPaused = reduce;
  var offscreen = false;
  var simNow = 0;
  var lastFrame = null;
  var cards,
    later,
    merged,
    prCount,
    cursorBusy,
    cursorPos,
    clearing,
    waiters,
    colEls,
    cardEls;

  COLS.forEach(function (c) {
    var col = document.createElement('div');
    col.className = 'col';
    col.dataset.col = c.id;
    col.innerHTML =
      '<div class="col-head"><b><span class="dot ' +
      c.dot +
      '"></span>' +
      c.name +
      ' <em class="num" data-count></em></b><small>' +
      c.help +
      '</small></div>';
    colsEl.appendChild(col);
  });
  colEls = {};
  colsEl.querySelectorAll('.col').forEach(function (el) {
    colEls[el.dataset.col] = el;
  });

  function reset() {
    cards = SEED.map(function (s) {
      return Object.assign({ since: -(s.age || 0), round: 1, rounds: 1 }, s);
    });
    later = LATER.slice();
    merged = 0;
    prCount = 1;
    cursorBusy = false;
    clearing = false;
    waiters = [];
    simNow = 0;
    cursorPos = { x: 140, y: 420 };
    Object.values(colEls).forEach(function (col) {
      col.querySelectorAll('.k').forEach(function (k) {
        k.remove();
      });
    });
    cardEls = {};
    render(false);
  }

  function wait(ms) {
    return new Promise(function (resolve) {
      waiters.push({ at: simNow + ms, resolve: resolve });
    });
  }
  function playing() {
    return !userPaused && !offscreen;
  }
  function minutes(c) {
    return Math.max(1, Math.round((simNow - c.since) / 650));
  }
  function isOpenBug(c) {
    return c.type === 'bug' && c.phase !== 'merged' && c.phase !== 'gone';
  }
  function needsYou(c) {
    return c.phase === 'gate' || c.phase === 'triaged' || c.phase === 'merged';
  }

  var CLOCK =
    '<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true"><circle cx="8" cy="8" r="6" /><path d="M8 4.8V8l2.2 1.4" /></svg>';
  function cardHtml(c) {
    var strip = '';
    var chips =
      c.type === 'bug'
        ? '<span class="chip bug">bug</span>'
        : '<span class="chip inv">investigation</span>';
    var status = '';
    var actions = '';
    switch (c.phase) {
      case 'filed':
        status =
          '<div class="k-status idle">' +
          CLOCK +
          'Filed by the API from pantry-feedback</div>';
        break;
      case 'planning':
        status =
          '<div class="k-status working"><span class="dot"></span>' +
          (c.type === 'inv' ? 'Investigating' : 'Planning') +
          ' · ' +
          minutes(c) +
          ' min</div>';
        break;
      case 'gate':
        strip =
          '<div class="k-strip"><span class="dot amber"></span>Needs you · the RCA is ready</div>';
        status =
          '<div class="k-status idle">Confidence high · no open questions</div>';
        actions =
          '<div class="k-actions"><span class="fbtn primary" data-act>Approve</span><span class="fbtn">Review</span></div>';
        break;
      case 'self':
        status =
          '<div class="k-status done"><span class="dot green"></span>Confidence high · approved itself</div>';
        break;
      case 'triaged':
        strip =
          '<div class="k-strip"><span class="dot amber"></span>Needs you</div>';
        status =
          '<div class="k-status attn"><span class="dot amber"></span>Triaged — report ready</div>';
        actions =
          '<div class="k-actions"><span class="fbtn primary" data-act>Convert to bug</span><span class="fbtn">Close</span></div>';
        break;
      case 'writing':
        status =
          '<div class="k-status working"><span class="dot"></span>' +
          (c.round > 1 ? 'Fixing review findings' : 'Writing the fix') +
          ' · ' +
          minutes(c) +
          ' min</div>';
        break;
      case 'ready':
        status =
          '<div class="k-status done"><span class="dot green"></span>Reproduction passes · tests added</div>';
        break;
      case 'reviewing':
        status =
          '<div class="k-status working"><span class="dot"></span>Reviewing · fix round ' +
          c.round +
          '</div>';
        break;
      case 'fixing':
        status =
          '<div class="k-status working"><span class="dot"></span>Fixing review findings · round ' +
          c.round +
          '</div>';
        break;
      case 'approved':
        status =
          '<div class="k-status done"><span class="dot green"></span><code>REVIEW: APPROVE</code></div>';
        break;
      case 'open':
        chips += '<span class="chip pr">PR open</span>';
        status =
          '<div class="k-status idle">' +
          CLOCK +
          'PR #' +
          c.pr +
          ' waiting for a merge</div>';
        break;
      case 'merged':
        strip =
          '<div class="k-strip"><span class="dot amber"></span>Merged — close card</div>';
        chips += '<span class="chip merged">Merged</span>';
        status =
          '<div class="k-status done"><span class="dot green"></span>PR #' +
          c.pr +
          ' merged just now</div>';
        actions =
          '<div class="k-actions"><span class="fbtn" data-act>Clean up</span></div>';
        break;
    }
    return (
      strip +
      '<div class="k-body"><div class="k-top"><span class="k-key">' +
      c.key +
      '</span>' +
      chips +
      '</div><p class="k-title">' +
      c.title +
      '</p>' +
      status +
      actions +
      '</div>'
    );
  }

  // Render the board, then play every card from where it was to where it
  // is now (FLIP), so moves between columns and reflows inside one read
  // as motion rather than jumps.
  function render(animate) {
    var first = {};
    Object.keys(cardEls).forEach(function (id) {
      first[id] = cardEls[id].getBoundingClientRect();
    });
    cards.forEach(function (c) {
      var el = cardEls[c.key];
      if (!el) {
        el = document.createElement('article');
        el.className = 'k';
        cardEls[c.key] = el;
      }
      var html = cardHtml(c);
      if (el._html !== html) {
        el.innerHTML = html;
        el._html = html;
      }
      el.classList.toggle('need', needsYou(c));
      var col = colEls[c.col];
      if (el.parentNode !== col) col.appendChild(el);
    });
    Object.keys(cardEls).forEach(function (id) {
      if (
        !cards.some(function (c) {
          return c.key === id;
        })
      ) {
        cardEls[id].remove();
        delete cardEls[id];
      }
    });
    COLS.forEach(function (c) {
      colEls[c.id].querySelector('[data-count]').textContent = cards.filter(
        function (k) {
          return k.col === c.id;
        }
      ).length;
    });
    if (animate && !reduce) {
      Object.keys(cardEls).forEach(function (id) {
        var el = cardEls[id];
        var to = el.getBoundingClientRect();
        var from = first[id];
        if (!from) {
          el.animate(
            [
              { opacity: 0, transform: 'translateY(-10px)' },
              { opacity: 1, transform: 'none' },
            ],
            { duration: 500, easing: 'cubic-bezier(0.16, 1, 0.3, 1)' }
          );
          return;
        }
        var dx = from.left - to.left;
        var dy = from.top - to.top;
        if (Math.abs(dx) > 0.5 || Math.abs(dy) > 0.5) {
          el.animate(
            [
              { transform: 'translate(' + dx + 'px,' + dy + 'px)' },
              { transform: 'none' },
            ],
            {
              duration: Math.abs(dx) > 1 ? 720 : 420,
              easing: 'cubic-bezier(0.16, 1, 0.3, 1)',
            }
          );
        }
      });
    }
    tally();
  }

  function tally() {
    var open = cards.filter(isOpenBug).length;
    document.getElementById('t-open').textContent = open;
    document.getElementById('t-merged').textContent = merged;
    var bar = document.getElementById('t-bar');
    var total = Math.max(open + merged, 9);
    if (bar.children.length !== total) {
      bar.innerHTML = '';
      for (var i = 0; i < total; i++)
        bar.appendChild(document.createElement('i'));
    }
    Array.prototype.forEach.call(bar.children, function (seg, i) {
      seg.classList.toggle('off', i >= open);
    });
    var need = cards.filter(needsYou).length;
    document.getElementById('need-n').textContent = need;
    document.getElementById('need').classList.toggle('zero', need === 0);
    document.getElementById('card-count').textContent =
      cards.length + (cards.length === 1 ? ' card' : ' cards');
  }

  function go(c, phase, col) {
    c.phase = phase;
    c.since = simNow;
    if (col) c.col = col;
  }

  // The automatic side of each stage: agents finishing their work, and in
  // Prep mode the moves Prepcook makes itself.
  function step() {
    var changed = false;
    var prep = mode === 'prep';
    var inPlan = cards.filter(function (c) {
      return c.col === 'plan';
    }).length;
    while (later.length && simNow >= later[0].at) {
      var n = later.shift();
      cards.push({
        key: n.key,
        type: n.type,
        title: n.title,
        col: 'todo',
        phase: 'filed',
        since: simNow,
        round: 1,
        rounds: 1,
      });
      changed = true;
    }
    cards.forEach(function (c) {
      var age = simNow - c.since;
      if (c.phase === 'filed' && prep && inPlan < 2 && age > T.filedAuto) {
        go(c, 'planning', 'plan');
        inPlan++;
        changed = true;
      } else if (
        c.phase === 'planning' &&
        age > (c.type === 'inv' ? T.inv : T.planning)
      ) {
        go(c, c.type === 'inv' ? 'triaged' : prep ? 'self' : 'gate');
        changed = true;
      } else if (c.phase === 'self' && age > T.selfApprove) {
        go(c, 'writing', 'execute');
        changed = true;
      } else if (
        c.phase === 'writing' &&
        age > (c.round > 1 ? T.fixing : T.writing)
      ) {
        go(
          c,
          c.round > 1 ? 'reviewing' : 'ready',
          c.round > 1 ? 'review' : null
        );
        changed = true;
      } else if (c.phase === 'ready' && prep && age > T.readyAuto) {
        go(c, 'reviewing', 'review');
        changed = true;
      } else if (c.phase === 'reviewing' && age > T.reviewing) {
        if (c.round < c.rounds) {
          c.round++;
          go(c, 'fixing');
        } else go(c, 'approved');
        changed = true;
      } else if (c.phase === 'fixing' && age > T.fixing) {
        go(c, 'reviewing');
        changed = true;
      } else if (c.phase === 'approved' && prep && age > T.approvedAuto) {
        c.pr = ++prCount;
        go(c, 'open', 'shipped');
        changed = true;
      } else if (c.phase === 'open' && age > T.open) {
        go(c, 'merged');
        merged++;
        changed = true;
      } else if (
        c.phase === 'planning' ||
        c.phase === 'writing' ||
        c.phase === 'reviewing' ||
        c.phase === 'fixing'
      ) {
        if (Math.round(age / 650) !== Math.round((age - 250) / 650))
          changed = true;
      }
    });
    if (changed) render(true);
    if (!cursorBusy) person();
    if (!clearing && !later.length && cards.length === 0) {
      clearing = true;
      document.getElementById('clear').classList.add('show');
      wait(4200).then(function () {
        reset();
        document.getElementById('clear').classList.remove('show');
      });
    }
  }

  // What a person does next: clean up merged cards, decide plans and
  // triaged investigations, and in Chef mode move the cards.
  function person() {
    var prep = mode === 'prep';
    var pick = function (phase) {
      return cards.find(function (c) {
        return c.phase === phase;
      });
    };
    var inPlan = cards.filter(function (c) {
      return c.col === 'plan';
    }).length;
    var c;
    if ((c = pick('merged')))
      return act(c, 'click', function () {
        c.phase = 'gone';
        cards.splice(cards.indexOf(c), 1);
      });
    if (
      (c = cards.find(function (k) {
        return k.phase === 'gate' && simNow - k.since > T.read;
      }))
    )
      return act(
        c,
        'click',
        function () {
          go(c, 'writing', 'execute');
        },
        '.fbtn:not([data-act])'
      );
    if ((c = pick('triaged')))
      return act(c, 'click', function () {
        c.type = 'bug';
        go(c, 'writing', 'execute');
      });
    if (prep) return rest();
    if ((c = pick('approved')))
      return act(c, 'drag', function () {
        c.pr = ++prCount;
        go(c, 'open', 'shipped');
      });
    if ((c = pick('ready')))
      return act(c, 'drag', function () {
        go(c, 'reviewing', 'review');
      });
    if (
      inPlan < 2 &&
      (c = cards.find(function (k) {
        return k.phase === 'filed' && simNow - k.since > 1200;
      }))
    )
      return act(c, 'drag', function () {
        go(c, 'planning', 'plan');
      });
    rest();
  }

  function rest() {
    cursor.classList.add('rest');
  }

  function rel(el) {
    var a = app.getBoundingClientRect();
    var r = el.getBoundingClientRect();
    return {
      x: r.left - a.left + r.width / 2,
      y: r.top - a.top + r.height / 2,
      visible: r.right > a.left + 20 && r.left < a.right - 20,
    };
  }
  function moveCursor(to, ms) {
    var from = cursorPos;
    cursorPos = { x: to.x, y: to.y };
    var end = 'translate(' + to.x + 'px,' + to.y + 'px)';
    if (reduce || !ms) {
      cursor.style.transform = end;
      return Promise.resolve();
    }
    var anim = cursor.animate(
      [
        { transform: 'translate(' + from.x + 'px,' + from.y + 'px)' },
        { transform: end },
      ],
      {
        duration: ms,
        easing: 'cubic-bezier(0.45, 0, 0.2, 1)',
        fill: 'forwards',
      }
    );
    return anim.finished.then(function () {
      cursor.style.transform = end;
      anim.cancel();
    });
  }

  // On a board narrower than its five columns, bring the column being
  // worked into view, unless the visitor has been scrolling it.
  var userScrolledAt = -Infinity;
  ['pointerdown', 'wheel', 'touchstart'].forEach(function (type) {
    colsEl.addEventListener(
      type,
      function () {
        userScrolledAt = Date.now();
      },
      { passive: true }
    );
  });
  function reveal(colId) {
    if (
      colsEl.scrollWidth <= colsEl.clientWidth + 2 ||
      Date.now() - userScrolledAt < 8000
    )
      return Promise.resolve();
    var col = colEls[colId];
    var style = getComputedStyle(colsEl);
    var inset = Math.min(
      parseFloat(style.paddingLeft) || 0,
      (parseFloat(style.columnGap) || 12) - 4
    );
    var left = Math.max(
      0,
      col.getBoundingClientRect().left -
        colsEl.getBoundingClientRect().left +
        colsEl.scrollLeft -
        inset
    );
    if (Math.abs(colsEl.scrollLeft - left) < 4) return Promise.resolve();
    colsEl.scrollTo({ left: left, behavior: reduce ? 'auto' : 'smooth' });
    return wait(reduce ? 0 : 520);
  }

  function act(c, kind, apply, glance) {
    cursorBusy = true;
    cursor.classList.remove('rest');
    var startKey = c.key;
    var spot;
    return reveal(c.col)
      .then(function () {
        return wait(400);
      })
      .then(function () {
        var el = cardEls[c.key];
        var look = glance && el.querySelector(glance);
        if (!look) return;
        var at = rel(look);
        cursor.classList.toggle('away', !at.visible);
        return moveCursor(at.visible ? at : cursorPos, 650).then(function () {
          return wait(900);
        });
      })
      .then(function () {
        var el = cardEls[c.key];
        var target =
          kind === 'click'
            ? el.querySelector('[data-act]')
            : el.querySelector('.k-title');
        spot = rel(target || el);
        cursor.classList.toggle('away', !spot.visible);
        return moveCursor(spot.visible ? spot : cursorPos, glance ? 380 : 650);
      })
      .then(function () {
        if (
          !cards.some(function (k) {
            return k.key === startKey;
          })
        )
          return;
        if (kind === 'click') {
          var btn = cardEls[c.key].querySelector('[data-act]');
          if (btn) btn.classList.add('pressed');
          return wait(220).then(function () {
            apply();
            render(true);
          });
        }
        var card = cardEls[c.key];
        card.classList.add('lifted');
        return wait(260).then(function () {
          apply();
          render(true);
          if (colsEl.scrollWidth > colsEl.clientWidth + 2) {
            card.classList.remove('lifted');
            cursor.classList.add('away');
            return reveal(c.col);
          }
          var landed = rel(
            cardEls[c.key].querySelector('.k-title') || cardEls[c.key]
          );
          return moveCursor(landed.visible ? landed : cursorPos, 720).then(
            function () {
              card.classList.remove('lifted');
            }
          );
        });
      })
      .then(function () {
        return wait(350);
      })
      .then(function () {
        cursorBusy = false;
      });
  }

  function frame(t) {
    var dt = lastFrame === null ? 0 : Math.min(t - lastFrame, 100);
    lastFrame = t;
    if (playing()) {
      simNow += dt;
      for (var i = waiters.length - 1; i >= 0; i--) {
        if (simNow >= waiters[i].at) waiters.splice(i, 1)[0].resolve();
      }
      stepAcc += dt;
      if (stepAcc >= 250) {
        stepAcc = 0;
        step();
      }
    }
    requestAnimationFrame(frame);
  }
  var stepAcc = 0;

  function syncPlaying() {
    var p = playing();
    document.getAnimations().forEach(function (a) {
      if (app.contains(a.effect && a.effect.target)) p ? a.play() : a.pause();
    });
    playBtn.setAttribute('aria-pressed', userPaused ? 'true' : 'false');
    playBtn.querySelector('span').textContent = userPaused ? 'Play' : 'Pause';
    playBtn.querySelector('svg').innerHTML = userPaused
      ? '<path d="M4.5 3.2v9.6a.6.6 0 0 0 .9.5l7.6-4.8a.6.6 0 0 0 0-1L5.4 2.7a.6.6 0 0 0-.9.5Z" fill="currentColor" />'
      : '<rect x="3.5" y="3" width="3" height="10" rx="1" fill="currentColor" /><rect x="9.5" y="3" width="3" height="10" rx="1" fill="currentColor" />';
  }
  playBtn.addEventListener('click', function () {
    userPaused = !userPaused;
    syncPlaying();
  });
  new IntersectionObserver(
    function (entries) {
      offscreen = !entries[0].isIntersecting;
      syncPlaying();
    },
    { threshold: 0.15 }
  ).observe(app);
  document.addEventListener('visibilitychange', function () {
    offscreen = document.hidden;
    syncPlaying();
  });

  function setMode(next) {
    mode = next;
    var prep = mode === 'prep';
    document.querySelectorAll('[data-mode]').forEach(function (b) {
      b.setAttribute(
        'aria-pressed',
        b.dataset.mode === mode ? 'true' : 'false'
      );
    });
    document
      .getElementById('mode-switch')
      .setAttribute('aria-checked', prep ? 'true' : 'false');
    document.getElementById('mode-name').textContent = prep
      ? 'Prep mode'
      : 'Chef mode';
    document.getElementById('mode-text').textContent = prep
      ? 'Prepcook moves the cards. Plans at high confidence approve themselves.'
      : 'Prepcook does the prep. You approve plans and move the cards.';
    // A plan waiting at the gate when Prep mode comes on clears the bar
    // and approves itself.
    if (prep) {
      cards.forEach(function (c) {
        if (c.phase === 'gate') go(c, 'self');
      });
      render(true);
    }
  }
  document.querySelectorAll('[data-mode]').forEach(function (b) {
    b.addEventListener('click', function () {
      setMode(b.dataset.mode);
    });
  });
  document.getElementById('mode-switch').addEventListener('click', function () {
    setMode(mode === 'prep' ? 'chef' : 'prep');
  });

  reset();
  syncPlaying();
  requestAnimationFrame(frame);
})();
