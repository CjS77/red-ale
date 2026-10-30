// red-ale landing page — demo walkthrough tabs
(function () {
  // Load the wordmark parts (red, ale, dot) from assets/wordmark.svg and inject
  // them into every .wordmark, so CSS can colour each part separately.
  (function () {
    fetch('/assets/wordmark.svg')
      .then(function (r) { return r.text(); })
      .then(function (text) {
        var wm = new DOMParser().parseFromString(text, 'image/svg+xml').querySelector('.wm');
        if (!wm) return;
        document.querySelectorAll('.wordmark').forEach(function (svg) {
          wm.childNodes.forEach(function (n) {
            if (n.nodeType === 1) svg.appendChild(n.cloneNode(true));
          });
        });
      });
  })();

  var STEPS = [
    {
      num: '01', label: 'Open a new book',
      code: [
        '$ mkdir red-ale-demo && cd red-ale-demo',
        '$ red-ale exec --create --book home.gnucash book.info'
      ].join('\n'),
      output: [
      "{",
      "  \"path\": \"/home/user/red-ale-demo/home.gnucash\",",
      "  \"guid\": \"f79e55fed0f85ab3e763079101c2d97a\",",
      "...",
      "  \"counts\": {",
      "    \"commodities\": 1,",
      "    \"accounts\": 1,",
      "    \"transactions\": 0,",
      "    \"orders\": 0,",
      "    \"taxTables\": 0,",
      "  }",
      "}"
      ].join("\n"), outLabel: 'Output',
      note: '<code>exec</code> opens a book, runs a method on it, saves if anything changed, and closes it.\n\nHere it creates home.gnucash and describes it: an empty book with only a root account.'
    },
    {
      num: '02', label: 'Chart of accounts',
      code: [
        '$ red-ale exec --book home.gnucash account.create \\',
        '    name=Assets type=ASSET commodity=USD placeholder:=true',
        '',
        '# setup.txt: one method per line, params in JSON',
        'account.create {"name": "Checking", "type": "BANK", "parent": "Assets"}',
        'account.create {"name": "Income", "type": "INCOME", "commodity": "USD", "placeholder": true}',
        'account.create {"name": "Salary", "type": "INCOME", "parent": "Income"}',
        'account.create {"name": "Expenses", "type": "EXPENSE", "commodity": "USD", "placeholder": true}',
        'account.create {"name": "Groceries", "type": "EXPENSE", "parent": "Expenses"}',
        '',
        '$ red-ale exec --book home.gnucash --script setup.txt',
        "$ red-ale exec --book home.gnucash account.list | jq -r '.accounts[].fullName'"
      ].join('\n'),
      output: ['Assets', 'Assets:Checking', 'Income', 'Income:Salary', 'Expenses', 'Expenses:Groceries'].join('\n'),
      outLabel: 'Output',
      note: 'Params are key=value for text and key:=value for anything else: true, a number, a list.\n\nA script is all or nothing. If one line fails, it stops and nothing is saved. Accounts are named by full name, with : between levels.'
    },
    {
      num: '03', label: 'Post transactions',
      code: [
        '$ red-ale exec --book home.gnucash transaction.post description="Pay day" datePosted=2024-07-01 \\',
        '    \'splits:=[{"account": "Assets:Checking", "value": "2500.00"}, {"account": "Income:Salary", "value": "-2500.00"}]\'',
        '',
        '$ red-ale exec --book home.gnucash transaction.post description="Corner shop" datePosted=2024-07-03 \\',
        '    \'splits:=[{"account": "Expenses:Groceries", "value": "42.17"}, {"account": "Assets:Checking", "value": "-42.17"}]\''
      ].join('\n'),
      output: [
        '{',
        '  "error": {',
        '    "code": 2002,',
        '    "message": "transaction is unbalanced by 1.00 USD",',
        '    "data": { "imbalance": "1.00", "currency": "USD" }',
        '  }',
        '}'
      ].join('\n'),
      outLabel: "What you get if the splits don't add up",
      note: 'Every transaction has two or more splits that must sum to zero. Positive is a debit, negative a credit, as in GnuCash.\n\nAmounts are always strings ("42.17"), exact to the cent. A bare number is refused, and so is anything that doesn\'t balance.'
    },
    {
      num: '04', label: 'Check the balance',
      code: [
        '$ red-ale exec --book home.gnucash account.balance account=Assets:Checking \\',
        '    | jq -c .balances.balance',
        '',
        '$ red-ale exec --book home.gnucash transaction.list account=Assets:Checking \\',
        '    | jq -r \'.transactions[] | "\\(.datePosted)  \\(.description)"\''
      ].join('\n'),
      output: [
        '{"num":"245783","denom":"100"}',
        '',
        '2024-07-01  Pay day',
        '2024-07-03  Corner shop'
      ].join('\n'),
      outLabel: 'Output',
      note: 'Amounts come back as fractions, the way GnuCash stores them: 245783/100 is 2,457.83.\n\nNow open home.gnucash in GnuCash. The same accounts, transactions and balances are there.'
    },
    {
      num: '05', label: 'Run it as a server',
      code: [
        '$ red-ale serve --book home.gnucash',
        '',
        '# in another terminal',
        '$ red-ale call book.info',
        '$ red-ale call price.add commodity=EUR currency=USD value=1.08 date=2024-07-01',
        '$ red-ale call book.save',
        '',
        '# or any JSON-RPC client',
        '$ curl -s http://127.0.0.1:7878 -H \'Content-Type: application/json\' \\',
        '    -d \'{"jsonrpc": "2.0", "id": 1, "method": "account.balance", "params": {"account": "Assets:Checking"}}\''
      ].join('\n'),
      output: '', outLabel: '',
      note: 'The server keeps the book open for many requests. Changes stay in memory until book.save.\n\nCtrl-C closes the book and releases the lock but does not save; the .log journal keeps the changes and book.replayJournal brings them back.\n\nIt listens on 127.0.0.1 only, with no password. Don\'t expose it to a network.'
    }
  ];

  // The walkthrough is the landing page's alone; the other pages only need the wordmark.
  var tabs = document.querySelectorAll('.tab');
  var el = {
    num: document.querySelectorAll('[data-step-num]'),
    label: document.querySelector('[data-step-label]'),
    code: document.getElementById('demo-code'),
    out: document.getElementById('demo-out'),
    outLabel: document.getElementById('demo-out-label'),
    outText: document.getElementById('demo-out-text'),
    note: document.getElementById('demo-note'),
    prev: document.getElementById('demo-prev'),
    next: document.getElementById('demo-next')
  };
  if (!tabs.length || !el.code || !el.label || !el.note || !el.out || !el.prev || !el.next) return;
  var current = 0;

  function show(i) {
    current = (i + STEPS.length) % STEPS.length;
    var s = STEPS[current];
    tabs.forEach(function (t, n) { t.setAttribute('aria-selected', n === current ? 'true' : 'false'); });
    el.num.forEach(function (n) { n.textContent = s.num; });
    el.label.textContent = s.label;
    el.code.textContent = s.code;
    el.note.innerHTML = s.note;
    el.out.hidden = !s.output;
    el.outLabel.textContent = s.outLabel;
    el.outText.textContent = s.output;
  }

  tabs.forEach(function (t, n) { t.addEventListener('click', function () { show(n); }); });
  el.prev.addEventListener('click', function () { show(current - 1); });
  el.next.addEventListener('click', function () { show(current + 1); });
  show(0);
})();
